//! Microsoft Project XML import/export.
use chrono::{NaiveDate, NaiveDateTime};
use projectfcg::duration::{DurUnit, Lag};
use projectfcg::model::{ConstraintType, LinkType, Project, ResourceType, TaskMode, TaskType};
use projectfcg::mspdi::{from_xml, to_xml};
use projectfcg::ops::Field;
use projectfcg::schedule::schedule;

fn dt(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, min, 0).unwrap()
}

/// A project using most of what MSPDI can carry.
fn rich() -> Project {
    let mut p = projectfcg::app::sample::software_release();
    // resources of each type with rates, a resource calendar
    let a = p.add_resource("Alice Smith", ResourceType::Work);
    p.resources[a].std_rate = 62.5;
    p.resources[a].ovt_rate = 90.0;
    p.resources[a].cost_per_use = 25.0;
    p.resources[a].max_units = 0.5;
    p.resources[a].email = "alice@example.com".into();
    p.resources[a].group = "Dev".into();
    p.resources[a].exceptions.push(projectfcg::calendar::CalendarException { name: "Vacation".into(), start: NaiveDate::from_ymd_opt(2026, 11, 16).unwrap(), end: NaiveDate::from_ymd_opt(2026, 11, 18).unwrap(), shifts: vec![] });
    let m = p.add_resource("Concrete", ResourceType::Material);
    p.resources[m].material_label = "tons".into();
    p.resources[m].std_rate = 120.0;
    let c = p.add_resource("Travel", ResourceType::Cost);
    let _ = c;
    p.set_field_text(5, Field::ResourceNames, "Alice Smith").unwrap();
    p.set_field_text(6, Field::ResourceNames, "Alice Smith[50%],Concrete[3]").unwrap();
    p.set_field_text(7, Field::ResourceNames, "Travel").unwrap();
    let k = p.assignments.iter().position(|x| p.resources.iter().any(|r| r.uid == x.resource && r.name == "Travel")).unwrap();
    p.assignments[k].cost = 1234.56;
    // task fields
    p.tasks[5].fixed_cost = 500.0;
    p.tasks[5].notes = "API first <b>&</b> \"quoted\"".into();
    p.tasks[5].deadline = Some(dt(2026, 12, 4, 17, 0));
    p.tasks[5].priority = 800;
    p.tasks[6].task_type = TaskType::FixedWork;
    p.tasks[11].constraint = ConstraintType::SNET;
    p.tasks[11].constraint_date = Some(dt(2026, 12, 14, 8, 0));
    // a percent lag and an elapsed lag
    let pl = Lag::parse("50%", &p.settings.durations).unwrap();
    let el = Lag::parse("2ed", &p.settings.durations).unwrap();
    p.add_link(10, 12, LinkType::FF, pl).unwrap();
    p.add_link(14, 15, LinkType::SS, el).unwrap();
    // manual task
    p.tasks[16].mode = TaskMode::Manual;
    p.tasks[16].manual_start = Some(dt(2027, 1, 20, 8, 0));
    schedule(&mut p);
    // baseline + progress
    p.set_baseline_n(0, None);
    p.set_baseline_n(3, None);
    p.set_percent_complete(1, 100.0).unwrap();
    p.set_percent_complete(5, 40.0).unwrap();
    schedule(&mut p);
    p
}

#[test]
fn round_trip_keeps_the_schedule() {
    let p = rich();
    let xml = to_xml(&p);
    let (q, warnings) = from_xml(&xml).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(q.title, p.title);
    assert_eq!(q.manager, p.manager);
    assert_eq!(q.start, p.start);
    assert_eq!(q.calendar.week, p.calendar.week);
    assert_eq!(q.calendar.exceptions.len(), p.calendar.exceptions.len());
    assert_eq!(q.tasks.len(), p.tasks.len());
    for (a, b) in p.tasks.iter().zip(&q.tasks) {
        assert_eq!((a.uid, &a.name, a.outline_level), (b.uid, &b.name, b.outline_level));
        assert_eq!((a.calc.start, a.calc.finish), (b.calc.start, b.calc.finish), "{}", a.name);
        if !a.calc.is_summary {
            assert_eq!(a.duration, b.duration, "{}", a.name);
        }
        assert_eq!(a.predecessors, b.predecessors, "{}", a.name);
        assert_eq!((a.constraint, a.constraint_date, a.deadline), (b.constraint, b.constraint_date, b.deadline), "{}", a.name);
        assert_eq!((a.mode, a.task_type, a.priority, a.milestone), (b.mode, b.task_type, b.priority, b.milestone), "{}", a.name);
        assert_eq!((a.actual_start, a.actual_finish), (b.actual_start, b.actual_finish), "{}", a.name);
        assert!((a.fixed_cost - b.fixed_cost).abs() < 0.005);
        assert_eq!(a.notes, b.notes);
        assert_eq!(a.baseline, b.baseline, "{}", a.name);
        assert_eq!(a.baselines[2], b.baselines[2], "baseline 3 of {}", a.name);
        if !a.calc.is_summary {
            assert!((a.percent_complete - b.percent_complete).abs() < 0.5, "{}", a.name);
        }
    }
    assert_eq!(q.resources.len(), p.resources.len());
    for (a, b) in p.resources.iter().zip(&q.resources) {
        assert_eq!((a.uid, &a.name, a.rtype), (b.uid, &b.name, b.rtype));
        assert_eq!((a.std_rate, a.ovt_rate, a.cost_per_use, a.max_units), (b.std_rate, b.ovt_rate, b.cost_per_use, b.max_units), "{}", a.name);
        assert_eq!((&a.email, &a.group, &a.material_label), (&b.email, &b.group, &b.material_label));
        assert_eq!(a.exceptions, b.exceptions, "{}", a.name);
    }
    assert_eq!(q.assignments, p.assignments);
    assert!((q.calc.total_cost - p.calc.total_cost).abs() < 0.01, "{} vs {}", q.calc.total_cost, p.calc.total_cost);
    assert_eq!(q.calc.finish, p.calc.finish);
    // and the second trip is byte-identical
    assert_eq!(to_xml(&q), xml);
}

#[test]
fn units_match_project() {
    let p = rich();
    let xml = to_xml(&p);
    // 10-day task: 80 working hours
    assert!(xml.contains("<Name>Backend API</Name>"));
    assert!(xml.contains("<Duration>PT80H0M0S</Duration>"));
    // 3d SS lag = 3 × 480 min × 10
    assert!(xml.contains("<LinkLag>14400</LinkLag>"), "lag in tenths of minutes");
    // percent lag is written as the plain percentage, format 19
    assert!(xml.contains("<LinkLag>50</LinkLag>\n        <LagFormat>19</LagFormat>"));
    // elapsed days lag: 2 × 1440 × 10, format 8 (ed)
    assert!(xml.contains("<LinkLag>28800</LinkLag>\n        <LagFormat>8</LagFormat>"));
    // currency in cents, rates per hour
    assert!(xml.contains("<FixedCost>50000</FixedCost>"));
    assert!(xml.contains("<CostPerUse>2500</CostPerUse>"));
    assert!(xml.contains("<StandardRate>62.5</StandardRate>"));
    assert!(xml.contains("<Cost>123456</Cost>"));
    // escaping
    assert!(xml.contains("API first &lt;b&gt;&amp;&lt;/b&gt; &quot;quoted&quot;"));
    // cost resources: Project writes material + IsCostResource
    assert!(xml.contains("<Name>Travel</Name>\n      <Type>0</Type>"));
    assert!(xml.contains("<IsCostResource>1</IsCostResource>"));
}

/// Element orders from the MSPDI schema (as in MPXJ's generated classes):
/// Project validates element order on import.
const ORDER_PROJECT: &str = "SaveVersion UID Name GUID Title Subject Category Company Manager Author CreationDate Revision LastSaved ScheduleFromStart StartDate FinishDate FYStartDate CriticalSlackLimit CurrencyDigits CurrencySymbol CurrencyCode CurrencySymbolPosition CalendarUID DefaultStartTime DefaultFinishTime MinutesPerDay MinutesPerWeek DaysPerMonth DefaultTaskType DefaultFixedCostAccrual DefaultStandardRate DefaultOvertimeRate DurationFormat WorkFormat EditableActualCosts HonorConstraints EarnedValueMethod InsertedProjectsLikeSummary MultipleCriticalPaths NewTasksEffortDriven NewTasksEstimated SplitsInProgressTasks SpreadActualCost SpreadPercentComplete TaskUpdatesResource FiscalYearStart WeekStartDay MoveCompletedEndsBack MoveRemainingStartsBack MoveRemainingStartsForward MoveCompletedEndsForward BaselineForEarnedValue AutoAddNewResourcesAndTasks StatusDate CurrentDate MicrosoftProjectServerURL Autolink NewTaskStartDate NewTasksAreManual DefaultTaskEVMethod ProjectExternallyEdited ExtendedCreationDate ActualsInSync RemoveFileProperties AdminProject BaselineCalendar UpdateManuallyScheduledTasksWhenEditingLinks KeepTaskOnNearestWorkingTimeWhenMadeAutoScheduled OutlineCodes WBSMasks ExtendedAttributes Calendars Tasks Resources Assignments";
const ORDER_TASK: &str = "UID GUID ID Name Active Manual Type IsNull CreateDate Contact WBS WBSLevel OutlineNumber OutlineLevel Priority Start Finish Duration DurationFormat Work Stop Resume ResumeValid EffortDriven Recurring OverAllocated Estimated Milestone Summary DisplayAsSummary Critical IsSubproject IsSubprojectReadOnly SubprojectName ExternalTask ExternalTaskProject EarlyStart EarlyFinish LateStart LateFinish StartVariance FinishVariance WorkVariance FreeSlack TotalSlack StartSlack FinishSlack FixedCost FixedCostAccrual PercentComplete PercentWorkComplete Cost OvertimeCost OvertimeWork ActualStart ActualFinish ActualDuration ActualCost ActualOvertimeCost ActualWork ActualOvertimeWork RegularWork RemainingDuration RemainingCost RemainingWork RemainingOvertimeCost RemainingOvertimeWork ACWP CV ConstraintType CalendarUID ConstraintDate Deadline LevelAssignments LevelingCanSplit LevelingDelay LevelingDelayFormat PreLeveledStart PreLeveledFinish Hyperlink HyperlinkAddress HyperlinkSubAddress IgnoreResourceCalendar Notes HideBar Rollup BCWS BCWP PhysicalPercentComplete EarnedValueMethod PredecessorLink ActualWorkProtected ActualOvertimeWorkProtected ExtendedAttribute Baseline OutlineCode IsPublished StatusManager CommitmentStart CommitmentFinish CommitmentType StartText FinishText DurationText ManualStart ManualFinish ManualDuration TimephasedData";
const ORDER_RESOURCE: &str = "UID GUID ID Name Type IsNull Initials Phonetics NTAccount MaterialLabel Code Group WorkGroup EmailAddress Hyperlink HyperlinkAddress HyperlinkSubAddress MaxUnits PeakUnits OverAllocated AvailableFrom AvailableTo Start Finish CanLevel AccrueAt Work RegularWork OvertimeWork ActualWork RemainingWork ActualOvertimeWork RemainingOvertimeWork PercentWorkComplete StandardRate StandardRateFormat Cost OvertimeRate OvertimeRateFormat OvertimeCost CostPerUse ActualCost ActualOvertimeCost RemainingCost RemainingOvertimeCost WorkVariance CostVariance SV CV ACWP CalendarUID Notes BCWS BCWP IsGeneric IsInactive IsEnterprise BookingType ActualWorkProtected ActualOvertimeWorkProtected ActiveDirectoryGUID CreationDate ExtendedAttribute Baseline OutlineCode CostCenter IsCostResource AssnOwner AssnOwnerGuid IsBudget AvailabilityPeriods Rates TimephasedData";
const ORDER_ASSIGNMENT: &str = "UID GUID TaskUID ResourceUID PercentWorkComplete ActualCost ActualFinish ActualOvertimeCost ActualOvertimeWork ActualStart ActualWork ACWP Confirmed Cost CostRateTable CostVariance CV Delay Finish FinishVariance Hyperlink HyperlinkAddress HyperlinkSubAddress WorkVariance HasFixedRateUnits FixedMaterial LevelingDelay LevelingDelayFormat LinkedFields Milestone Notes OverAllocated OvertimeCost OvertimeWork PeakUnits RateScale RegularWork RemainingCost RemainingOvertimeCost RemainingOvertimeWork RemainingWork ResponsePending Start Stop Resume StartVariance Summary SV Units UpdateNeeded VAC Work WorkContour BCWS BCWP BookingType ActualWorkProtected ActualOvertimeWorkProtected CreationDate AssnOwner AssnOwnerGuid BudgetCost BudgetWork ExtendedAttribute Baseline TimephasedData";
const ORDER_CALENDAR: &str = "UID GUID Name IsBaseCalendar IsBaselineCalendar BaseCalendarUID WeekDays Exceptions WorkWeeks";
const ORDER_EXCEPTION: &str = "EnteredByOccurrences TimePeriod Occurrences Name Type Period DaysOfWeek MonthItem MonthPosition Month MonthDay DayWorking WorkingTimes";
const ORDER_WEEKDAY: &str = "DayType DayWorking TimePeriod WorkingTimes";
const ORDER_LINK: &str = "PredecessorUID Type CrossProject CrossProjectName LinkLag LagFormat";
const ORDER_TASK_BASELINE: &str = "TimephasedData Number Interim Start Finish Duration DurationFormat EstimatedDuration Work Cost BCWS BCWP FixedCost";

fn check_order(node: roxmltree::Node, order: &str, what: &str) {
    let pos: Vec<&str> = order.split_whitespace().collect();
    let mut last = 0usize;
    for c in node.children().filter(|c| c.is_element()) {
        let name = c.tag_name().name();
        let i = pos.iter().position(|p| *p == name).unwrap_or_else(|| panic!("{what}: <{name}> isn't in the schema"));
        assert!(i >= last, "{what}: <{name}> is out of order");
        last = i;
    }
}

#[test]
fn element_order_follows_the_schema() {
    let p = rich();
    let xml = to_xml(&p);
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let root = doc.root_element();
    assert_eq!(root.tag_name().namespace(), Some("http://schemas.microsoft.com/project"));
    check_order(root, ORDER_PROJECT, "Project");
    fn el<'a, 'i>(n: roxmltree::Node<'a, 'i>, name: &str) -> Vec<roxmltree::Node<'a, 'i>> {
        n.descendants().filter(|c| c.is_element() && c.tag_name().name() == name).collect()
    }
    for t in el(root, "Task") {
        check_order(t, ORDER_TASK, "Task");
        for l in t.children().filter(|c| c.tag_name().name() == "PredecessorLink") {
            check_order(l, ORDER_LINK, "PredecessorLink");
        }
        for b in t.children().filter(|c| c.tag_name().name() == "Baseline") {
            check_order(b, ORDER_TASK_BASELINE, "Task/Baseline");
        }
    }
    for r in el(root, "Resource") {
        check_order(r, ORDER_RESOURCE, "Resource");
    }
    for a in el(root, "Assignment") {
        check_order(a, ORDER_ASSIGNMENT, "Assignment");
    }
    for c in el(root, "Calendar") {
        check_order(c, ORDER_CALENDAR, "Calendar");
    }
    for e in el(root, "Exception") {
        check_order(e, ORDER_EXCEPTION, "Exception");
    }
    for w in el(root, "WeekDay") {
        check_order(w, ORDER_WEEKDAY, "WeekDay");
    }
    assert!(!el(root, "Exception").is_empty() && el(root, "Calendar").len() >= 2);
}

/// What Project 2016 writes, trimmed: the project summary task (UID 0),
/// recurring holidays, a manually scheduled task, a cross-project link.
const FROM_PROJECT: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Project xmlns="http://schemas.microsoft.com/project">
  <SaveVersion>14</SaveVersion>
  <Name>Office move.xml</Name>
  <Title>Office move</Title>
  <ScheduleFromStart>1</ScheduleFromStart>
  <StartDate>2027-01-04T08:00:00</StartDate>
  <CalendarUID>1</CalendarUID>
  <DefaultStartTime>08:00:00</DefaultStartTime>
  <DefaultFinishTime>17:00:00</DefaultFinishTime>
  <MinutesPerDay>480</MinutesPerDay>
  <MinutesPerWeek>2400</MinutesPerWeek>
  <DaysPerMonth>20</DaysPerMonth>
  <NewTasksAreManual>0</NewTasksAreManual>
  <Calendars>
    <Calendar>
      <UID>1</UID>
      <Name>Standard</Name>
      <IsBaseCalendar>1</IsBaseCalendar>
      <BaseCalendarUID>-1</BaseCalendarUID>
      <WeekDays>
        <WeekDay><DayType>1</DayType><DayWorking>0</DayWorking></WeekDay>
        <WeekDay><DayType>2</DayType><DayWorking>1</DayWorking><WorkingTimes><WorkingTime><FromTime>08:00:00</FromTime><ToTime>12:00:00</ToTime></WorkingTime><WorkingTime><FromTime>13:00:00</FromTime><ToTime>17:00:00</ToTime></WorkingTime></WorkingTimes></WeekDay>
        <WeekDay><DayType>3</DayType><DayWorking>1</DayWorking><WorkingTimes><WorkingTime><FromTime>08:00:00</FromTime><ToTime>12:00:00</ToTime></WorkingTime><WorkingTime><FromTime>13:00:00</FromTime><ToTime>17:00:00</ToTime></WorkingTime></WorkingTimes></WeekDay>
        <WeekDay><DayType>4</DayType><DayWorking>1</DayWorking><WorkingTimes><WorkingTime><FromTime>08:00:00</FromTime><ToTime>12:00:00</ToTime></WorkingTime><WorkingTime><FromTime>13:00:00</FromTime><ToTime>17:00:00</ToTime></WorkingTime></WorkingTimes></WeekDay>
        <WeekDay><DayType>5</DayType><DayWorking>1</DayWorking><WorkingTimes><WorkingTime><FromTime>08:00:00</FromTime><ToTime>12:00:00</ToTime></WorkingTime><WorkingTime><FromTime>13:00:00</FromTime><ToTime>17:00:00</ToTime></WorkingTime></WorkingTimes></WeekDay>
        <WeekDay><DayType>6</DayType><DayWorking>1</DayWorking><WorkingTimes><WorkingTime><FromTime>08:00:00</FromTime><ToTime>12:00:00</ToTime></WorkingTime></WorkingTimes></WeekDay>
        <WeekDay><DayType>7</DayType><DayWorking>0</DayWorking></WeekDay>
      </WeekDays>
      <Exceptions>
        <Exception>
          <EnteredByOccurrences>1</EnteredByOccurrences>
          <TimePeriod><FromDate>2027-01-01T00:00:00</FromDate><ToDate>2029-01-01T23:59:00</ToDate></TimePeriod>
          <Occurrences>3</Occurrences>
          <Name>New Year</Name>
          <Type>2</Type>
          <Month>0</Month>
          <MonthDay>1</MonthDay>
          <DayWorking>0</DayWorking>
        </Exception>
        <Exception>
          <EnteredByOccurrences>0</EnteredByOccurrences>
          <TimePeriod><FromDate>2027-01-01T00:00:00</FromDate><ToDate>2027-12-31T23:59:00</ToDate></TimePeriod>
          <Occurrences>12</Occurrences>
          <Name>Team day</Name>
          <Type>5</Type>
          <Period>1</Period>
          <MonthItem>4</MonthItem>
          <MonthPosition>4</MonthPosition>
          <DayWorking>0</DayWorking>
        </Exception>
        <Exception>
          <EnteredByOccurrences>0</EnteredByOccurrences>
          <TimePeriod><FromDate>2027-01-09T00:00:00</FromDate><ToDate>2027-01-09T23:59:00</ToDate></TimePeriod>
          <Occurrences>1</Occurrences>
          <Name>Move Saturday</Name>
          <Type>1</Type>
          <DayWorking>1</DayWorking>
          <WorkingTimes><WorkingTime><FromTime>09:00:00</FromTime><ToTime>13:00:00</ToTime></WorkingTime></WorkingTimes>
        </Exception>
      </Exceptions>
    </Calendar>
    <Calendar>
      <UID>3</UID>
      <Name>Movers</Name>
      <IsBaseCalendar>0</IsBaseCalendar>
      <BaseCalendarUID>1</BaseCalendarUID>
      <WeekDays>
        <WeekDay><DayType>7</DayType><DayWorking>1</DayWorking><WorkingTimes><WorkingTime><FromTime>08:00:00</FromTime><ToTime>00:00:00</ToTime></WorkingTime></WorkingTimes></WeekDay>
      </WeekDays>
    </Calendar>
  </Calendars>
  <Tasks>
    <Task><UID>0</UID><ID>0</ID><Name>Office move</Name><Type>1</Type><IsNull>0</IsNull><OutlineLevel>0</OutlineLevel><Summary>1</Summary></Task>
    <Task>
      <UID>7</UID><ID>1</ID><Name>Pack</Name><Active>1</Active><Manual>0</Manual><Type>0</Type><IsNull>0</IsNull>
      <OutlineNumber>1</OutlineNumber><OutlineLevel>1</OutlineLevel><Priority>500</Priority>
      <Start>2027-01-04T08:00:00</Start><Finish>2027-01-05T17:00:00</Finish>
      <Duration>PT16H0M0S</Duration><DurationFormat>39</DurationFormat><Estimated>1</Estimated>
      <PercentComplete>50</PercentComplete><ActualStart>2027-01-04T08:00:00</ActualStart>
      <ConstraintType>0</ConstraintType><CalendarUID>-1</CalendarUID>
    </Task>
    <Task>
      <UID>9</UID><ID>2</ID><Name>Truck</Name><Active>1</Active><Manual>0</Manual><Type>1</Type><IsNull>0</IsNull>
      <OutlineLevel>1</OutlineLevel><Start>2027-01-06T08:00:00</Start>
      <Duration>PT4H0M0S</Duration><DurationFormat>5</DurationFormat>
      <ConstraintType>4</ConstraintType><CalendarUID>-1</CalendarUID><ConstraintDate>2027-01-06T08:00:00</ConstraintDate>
      <Notes>Book the big one</Notes>
      <PredecessorLink><PredecessorUID>7</PredecessorUID><Type>1</Type><CrossProject>0</CrossProject><LinkLag>-2400</LinkLag><LagFormat>5</LagFormat></PredecessorLink>
      <PredecessorLink><PredecessorUID>99</PredecessorUID><Type>1</Type><CrossProject>1</CrossProject><CrossProjectName>Other.mpp\5</CrossProjectName><LinkLag>0</LinkLag><LagFormat>7</LagFormat></PredecessorLink>
    </Task>
    <Task>
      <UID>12</UID><ID>3</ID><Name>Unpack (whenever)</Name><Active>1</Active><Manual>1</Manual><Type>0</Type><IsNull>0</IsNull>
      <OutlineLevel>1</OutlineLevel><Start>2027-01-20T08:00:00</Start><Finish>2027-01-20T17:00:00</Finish>
      <Duration>PT8H0M0S</Duration><DurationFormat>7</DurationFormat>
      <ConstraintType>0</ConstraintType><CalendarUID>-1</CalendarUID>
      <ManualStart>2027-01-20T08:00:00</ManualStart><ManualFinish>2027-01-20T17:00:00</ManualFinish><ManualDuration>PT8H0M0S</ManualDuration>
    </Task>
    <Task><UID>13</UID><ID>4</ID><Name>Done</Name><IsNull>0</IsNull><OutlineLevel>1</OutlineLevel><Duration>PT0H0M0S</Duration><Milestone>1</Milestone>
      <PredecessorLink><PredecessorUID>9</PredecessorUID><Type>1</Type><LinkLag>0</LinkLag><LagFormat>7</LagFormat></PredecessorLink></Task>
  </Tasks>
  <Resources>
    <Resource><UID>0</UID><ID>0</ID><Type>1</Type><IsNull>0</IsNull></Resource>
    <Resource><UID>1</UID><ID>1</ID><Name>Movers</Name><Type>1</Type><IsNull>0</IsNull><MaxUnits>3.00</MaxUnits><AccrueAt>1</AccrueAt><StandardRate>40</StandardRate><StandardRateFormat>2</StandardRateFormat><CostPerUse>15000</CostPerUse><CalendarUID>3</CalendarUID></Resource>
    <Resource><UID>2</UID><ID>2</ID><Name>Boxes</Name><Type>0</Type><IsNull>0</IsNull><MaterialLabel>box</MaterialLabel><StandardRate>2</StandardRate></Resource>
  </Resources>
  <Assignments>
    <Assignment><UID>1</UID><TaskUID>9</TaskUID><ResourceUID>1</ResourceUID><Units>2</Units><Work>PT8H0M0S</Work></Assignment>
    <Assignment><UID>2</UID><TaskUID>7</TaskUID><ResourceUID>2</ResourceUID><Units>40</Units></Assignment>
    <Assignment><UID>3</UID><TaskUID>13</TaskUID><ResourceUID>-65535</ResourceUID><Units>1</Units></Assignment>
  </Assignments>
</Project>"#;

#[test]
fn reads_what_project_writes() {
    let (p, warnings) = from_xml(FROM_PROJECT).unwrap();
    assert_eq!(p.title, "Office move");
    assert_eq!(p.tasks.len(), 4, "the project summary task (UID 0) is skipped");
    // calendar: Friday half day, recurring holidays expanded
    assert_eq!(p.calendar.week[4].len(), 1);
    assert_eq!(p.calendar.week[4][0].end, 12 * 60);
    let ex = |name: &str| p.calendar.exceptions.iter().filter(|e| e.name == name).map(|e| e.start).collect::<Vec<_>>();
    let d = |y, m, dd| NaiveDate::from_ymd_opt(y, m, dd).unwrap();
    assert_eq!(ex("New Year"), vec![d(2027, 1, 1), d(2028, 1, 1), d(2029, 1, 1)]);
    let team = ex("Team day");
    assert_eq!(team.len(), 12);
    assert_eq!(team[0], d(2027, 1, 25), "last Monday of January");
    assert_eq!(team[1], d(2027, 2, 22));
    let sat = p.calendar.exceptions.iter().find(|e| e.name == "Move Saturday").unwrap();
    assert_eq!(sat.shifts.len(), 1);
    // tasks
    let pack = &p.tasks[0];
    assert_eq!((pack.uid, pack.duration.minutes, pack.duration.estimated), (7, 960, true));
    assert_eq!(pack.percent_complete, 50.0);
    assert_eq!(pack.actual_start, Some(dt(2027, 1, 4, 8, 0)));
    let truck = &p.tasks[1];
    assert_eq!(truck.task_type, TaskType::FixedDuration);
    assert_eq!(truck.duration.unit, DurUnit::Hour);
    assert_eq!(truck.constraint, ConstraintType::SNET);
    assert_eq!(truck.notes, "Book the big one");
    assert_eq!(truck.predecessors.len(), 1, "the cross-project link is dropped");
    assert_eq!(truck.predecessors[0].lag, Lag::Time { minutes: -240, unit: DurUnit::Hour, elapsed: false });
    let unpack = &p.tasks[2];
    assert_eq!(unpack.mode, TaskMode::Manual);
    assert_eq!(unpack.calc.start, Some(dt(2027, 1, 20, 8, 0)));
    assert!(p.tasks[3].is_milestone());
    // resources
    assert_eq!(p.resources.len(), 2, "UID 0 placeholder skipped");
    let movers = &p.resources[0];
    assert_eq!((movers.max_units, movers.std_rate, movers.cost_per_use), (3.0, 40.0, 150.0));
    let w = movers.week.as_ref().expect("own Saturday");
    assert_eq!(w[5], vec![projectfcg::calendar::Shift::new(480, 1440)], "midnight end");
    assert_eq!(w[0], p.calendar.week[0], "other days from the base");
    assert_eq!(p.resources[1].rtype, ResourceType::Material);
    assert_eq!(p.assignments.len(), 2, "the unassigned placeholder is skipped");
    assert!(warnings.iter().any(|w| w.contains("another project")), "{warnings:?}");
    assert!(p.calc.errors.is_empty(), "{:?}", p.calc.errors);
}

#[test]
fn rejects_non_project_xml() {
    assert!(from_xml("<html><body/></html>").is_err());
    assert!(from_xml("not xml").is_err());
    assert!(from_xml("<Project xmlns=\"http://schemas.microsoft.com/project\"></Project>").is_ok(), "an empty project is fine");
}

#[test]
fn agile_sample_exports_and_reports_board_loss_nothing_else() {
    let p = projectfcg::app::sample::agile_release();
    let (q, w) = from_xml(&to_xml(&p)).unwrap();
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(q.tasks.len(), p.tasks.len());
    assert_eq!(q.calc.finish, p.calc.finish);
    assert!(q.tickets.is_empty(), "the board isn't in MSPDI");
}

/// Files written by another MSPDI writer (MPXJ 16, which mirrors what
/// Project itself writes: project summary task, GUIDs, timephased data,
/// every calendar written out) from our exports of the samples. Reading
/// them back must give the same schedule.
#[test]
fn reads_files_from_another_writer() {
    let p = projectfcg::app::sample::software_release();
    let (q, w) = from_xml(include_str!("fixtures/software_release_mpxj.xml")).unwrap();
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(q.tasks.len(), p.tasks.len());
    for (a, b) in p.tasks.iter().zip(&q.tasks) {
        assert_eq!((&a.name, a.calc.start, a.calc.finish), (&b.name, b.calc.start, b.calc.finish));
        assert_eq!(a.predecessors.len(), b.predecessors.len(), "{}", a.name);
    }
    assert_eq!(q.calendar.exceptions.len(), p.calendar.exceptions.len());
    assert!((q.calc.total_cost - p.calc.total_cost).abs() < 0.01, "{} vs {}", q.calc.total_cost, p.calc.total_cost);
    assert_eq!(q.resources.len(), p.resources.len());
    let (q, w) = from_xml(include_str!("fixtures/agile_release_mpxj.xml")).unwrap();
    assert!(w.is_empty(), "{w:?}");
    assert!(q.calc.errors.is_empty());
    assert_eq!(q.tasks.iter().filter(|t| t.name.starts_with("Sprint")).count(), 4);
}

#[path = "common/mod.rs"]
mod common;

/// Damaged files never panic: each import either succeeds (and schedules)
/// or reports an error.
#[test]
fn damaged_files_never_panic() {
    for (seed, xml) in [(42, to_xml(&rich())), (43, FROM_PROJECT.to_string()), (44, include_str!("fixtures/agile_release_mpxj.xml").to_string())] {
        damage(seed, &xml);
    }
}

fn damage(seed: u64, xml: &str) {
    let xml = xml.to_string();
    let mut r = common::Rng::new(seed);
    let values: Vec<(usize, usize)> = {
        // spans of element text between '>' and '</'
        let b = xml.as_bytes();
        let mut v = vec![];
        let mut i = 0;
        while i + 1 < b.len() {
            if b[i] == b'>' && b[i + 1] != b'<' && b[i + 1] != b'\n' {
                if let Some(j) = xml[i + 1..].find("</") {
                    v.push((i + 1, i + 1 + j));
                }
            }
            i += 1;
        }
        v
    };
    let junk = ["", "-1", "0", "abc", "99999999999999", "-PT9999999H0M0S", "PT", "2026-02-30T08:00:00", "1e308", "NaN", "24:00:00", "-2147483648"];
    for _ in 0..1500 {
        let mut s = xml.clone();
        for _ in 0..1 + r.below(4) {
            let (a, b) = *r.pick(&values);
            if b <= s.len() {
                s.replace_range(a..b, r.pick(&junk));
            }
        }
        if let Ok((p, _)) = from_xml(&s) {
            assert!(p.tasks.iter().all(|t| t.outline_level >= 1));
        }
    }
    // truncated files
    for cut in (0..xml.len()).step_by(997) {
        let _ = from_xml(&xml[..cut]);
    }
}

#[test]
fn damaged_jira_csv_never_panics() {
    let mut p = rich();
    projectfcg::jira::import_csv(&mut p, "Summary,Issue Type\nE,Epic\nS,Story\n", &Default::default()).unwrap();
    let csv = {
        let mut q = projectfcg::app::sample::agile_release();
        q.board.key_prefix = "APP".into();
        projectfcg::jira::export_csv(&q)
    };
    let mut r = common::Rng::new(7);
    let lines: Vec<&str> = csv.lines().collect();
    for _ in 0..200 {
        // drop, duplicate and shuffle rows, and cut lines short
        let mut out = vec![lines[0].to_string()];
        for _ in 0..r.below(lines.len() as u64 * 2) {
            let l = *r.pick(&lines[1..]);
            let cut = r.below(l.len() as u64 + 1) as usize;
            let l = if r.chance(20) { l.get(..cut).unwrap_or(l) } else { l };
            out.push(l.to_string());
        }
        let text = out.join(if r.chance(50) { "\n" } else { "\r\n" });
        let mut q = projectfcg::model::Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
        let opts = projectfcg::jira::ImportOptions { create_sprints: r.chance(50), create_people: r.chance(50) };
        if projectfcg::jira::import_csv(&mut q, &text, &opts).is_ok() {
            schedule(&mut q);
            let mut ids: Vec<u32> = q.tickets.iter().map(|t| t.id).collect();
            ids.sort();
            assert!(ids.windows(2).all(|w| w[0] != w[1]), "duplicate ids");
        }
    }
}

/// Findings from the 0.6 review, pinned.
#[test]
fn review_regressions_calendars() {
    use projectfcg::calendar::{CalendarException, Shift};
    let d = |m, dd| NaiveDate::from_ymd_opt(2026, m, dd).unwrap();
    let wrap = |cal: &str| format!(r#"<?xml version="1.0"?><Project xmlns="http://schemas.microsoft.com/project"><StartDate>2026-01-05T08:00:00</StartDate><CalendarUID>2</CalendarUID><Calendars>{cal}</Calendars></Project>"#);
    // "last day of every month" and "first weekday of every month"
    let xml = wrap(r#"<Calendar><UID>2</UID><Name>S</Name><IsBaseCalendar>1</IsBaseCalendar><Exceptions>
        <Exception><TimePeriod><FromDate>2026-01-01T00:00:00</FromDate><ToDate>2026-03-31T23:59:00</ToDate></TimePeriod><Name>Close</Name><Type>5</Type><Period>1</Period><MonthItem>0</MonthItem><MonthPosition>4</MonthPosition><DayWorking>0</DayWorking></Exception>
        <Exception><TimePeriod><FromDate>2026-01-01T00:00:00</FromDate><ToDate>2026-03-31T23:59:00</ToDate></TimePeriod><Name>Plan</Name><Type>5</Type><Period>1</Period><MonthItem>1</MonthItem><MonthPosition>0</MonthPosition><DayWorking>0</DayWorking></Exception>
        </Exceptions></Calendar>"#);
    let (p, _) = from_xml(&xml).unwrap();
    let on = |n: &str| p.calendar.exceptions.iter().filter(|e| e.name == n).map(|e| e.start).collect::<Vec<_>>();
    assert_eq!(on("Close"), vec![d(1, 31), d(2, 28), d(3, 31)]);
    assert_eq!(on("Plan"), vec![d(1, 1), d(2, 2), d(3, 2)]);
    // a derived calendar's exception beats its base's on the same day
    let xml = wrap(r#"<Calendar><UID>1</UID><Name>Base</Name><IsBaseCalendar>1</IsBaseCalendar><Exceptions><Exception><TimePeriod><FromDate>2026-12-24T00:00:00</FromDate><ToDate>2026-12-24T23:59:00</ToDate></TimePeriod><Name>Eve</Name><Type>1</Type><DayWorking>0</DayWorking></Exception></Exceptions></Calendar>
        <Calendar><UID>2</UID><Name>Derived</Name><IsBaseCalendar>0</IsBaseCalendar><BaseCalendarUID>1</BaseCalendarUID><Exceptions><Exception><TimePeriod><FromDate>2026-12-24T00:00:00</FromDate><ToDate>2026-12-24T23:59:00</ToDate></TimePeriod><Name>Half day</Name><Type>1</Type><DayWorking>1</DayWorking><WorkingTimes><WorkingTime><FromTime>08:00:00</FromTime><ToTime>12:00:00</ToTime></WorkingTime></WorkingTimes></Exception></Exceptions></Calendar>"#);
    let (p, _) = from_xml(&xml).unwrap();
    assert_eq!(p.calendar.working_minutes_on(d(12, 24)), 240);
    // overlapping exceptions: written without overlaps, same working time back
    let mut p = Project::new(d(12, 1));
    p.calendar.exceptions.push(CalendarException { name: "On call".into(), start: d(12, 28), end: d(12, 28), shifts: vec![Shift::new(480, 720)] });
    p.calendar.exceptions.push(CalendarException { name: "Shutdown".into(), start: d(12, 21), end: NaiveDate::from_ymd_opt(2027, 1, 1).unwrap(), shifts: vec![] });
    schedule(&mut p);
    let (q, _) = from_xml(&to_xml(&p)).unwrap();
    for day in d(12, 18).iter_days().take(20) {
        assert_eq!(q.calendar.working_minutes_on(day), p.calendar.working_minutes_on(day), "{day}");
    }
    let ex = &q.calendar.exceptions;
    assert!(ex.iter().enumerate().all(|(i, a)| ex[i + 1..].iter().all(|b| a.end < b.start || b.end < a.start)), "no overlaps written");
    // a resource on the project calendar doesn't copy its holidays; 24:00 ends; DayWorking without times
    let xml = wrap(r#"<Calendar><UID>2</UID><Name>S</Name><IsBaseCalendar>1</IsBaseCalendar><WeekDays><WeekDay><DayType>7</DayType><DayWorking>1</DayWorking><WorkingTimes><WorkingTime><FromTime>20:00:00</FromTime><ToTime>24:00:00</ToTime></WorkingTime></WorkingTimes></WeekDay><WeekDay><DayType>1</DayType><DayWorking>1</DayWorking></WeekDay></WeekDays><Exceptions><Exception><TimePeriod><FromDate>2026-01-01T00:00:00</FromDate><ToDate>2026-01-01T23:59:00</ToDate></TimePeriod><Name>NY</Name><Type>1</Type><DayWorking>0</DayWorking></Exception></Exceptions></Calendar>"#)
        .replace("</Calendars>", "</Calendars><Resources><Resource><UID>1</UID><Name>R</Name><Type>1</Type><CalendarUID>2</CalendarUID></Resource></Resources><Assignments><Assignment><UID>1</UID><TaskUID>77</TaskUID><ResourceUID>1</ResourceUID></Assignment></Assignments>");
    let (p, w) = from_xml(&xml).unwrap();
    assert!(p.resources[0].exceptions.is_empty());
    assert_eq!(p.calendar.week[5], vec![Shift::new(1200, 1440)]);
    assert_eq!(p.calendar.week[6].iter().map(|s| s.minutes()).sum::<i64>(), 480);
    assert!(w.iter().any(|x| x.contains("assignment")), "{w:?}");
    // critical slack threshold rounds to whole days
    let mut p = Project::new(d(1, 5));
    p.settings.critical_slack_threshold = 2 * 480 - 60;
    assert!(to_xml(&p).contains("<CriticalSlackLimit>2</CriticalSlackLimit>"));
}
