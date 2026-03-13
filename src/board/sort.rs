use std::fmt;
use std::str::FromStr;

use crate::model::config::Config;
use crate::model::task::Task;

/// Enum of fields by which tasks can be sorted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortField {
    Id,
    Status,
    Priority,
    Created,
    Updated,
    Due,
}

impl Default for SortField {
    fn default() -> Self {
        SortField::Id
    }
}

impl FromStr for SortField {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "id" => Ok(SortField::Id),
            "status" => Ok(SortField::Status),
            "priority" => Ok(SortField::Priority),
            "created" => Ok(SortField::Created),
            "updated" => Ok(SortField::Updated),
            "due" => Ok(SortField::Due),
            _ => Err(format!(
                "invalid sort field {:?}; valid fields: id, status, priority, created, updated, due",
                s
            )),
        }
    }
}

impl fmt::Display for SortField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SortField::Id => "id",
            SortField::Status => "status",
            SortField::Priority => "priority",
            SortField::Created => "created",
            SortField::Updated => "updated",
            SortField::Due => "due",
        };
        write!(f, "{}", s)
    }
}

/// Stable-sorts tasks by the given field.
///
/// For `Status` and `Priority`, order is determined by the config-defined
/// index (not alphabetical). For `Due`, `None` sorts last.
pub fn sort(tasks: &mut [&Task], field: SortField, reverse: bool, cfg: &Config) {
    tasks.sort_by(|a, b| {
        let (left, right) = if reverse { (b, a) } else { (a, b) };
        compare_tasks(left, right, field, cfg)
    });
}

/// Also sorts owned `Task` slices (useful for list operations).
pub fn sort_owned(tasks: &mut [Task], field: SortField, reverse: bool, cfg: &Config) {
    tasks.sort_by(|a, b| {
        let (left, right) = if reverse { (b, a) } else { (a, b) };
        compare_tasks(left, right, field, cfg)
    });
}

fn compare_tasks(a: &Task, b: &Task, field: SortField, cfg: &Config) -> std::cmp::Ordering {
    match field {
        SortField::Id => a.id.cmp(&b.id),
        SortField::Status => cfg.status_index(&a.status).cmp(&cfg.status_index(&b.status)),
        SortField::Priority => cfg.priority_index(&a.priority).cmp(&cfg.priority_index(&b.priority)),
        SortField::Created => a.created.cmp(&b.created),
        SortField::Updated => a.updated.cmp(&b.updated),
        SortField::Due => compare_due(a, b),
    }
}

fn compare_due(a: &Task, b: &Task) -> std::cmp::Ordering {
    match (&a.due, &b.due) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Greater, // None sorts last
        (Some(_), None) => std::cmp::Ordering::Less,
        (Some(da), Some(db)) => da.cmp(db),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(id: i32, priority: &str, status: &str) -> Task {
        Task {
            id,
            title: format!("Task {}", id),
            status: status.to_string(),
            priority: priority.to_string(),
            ..Default::default()
        }
    }

    fn make_config() -> Config {
        Config::new_default("test")
    }

    #[test]
    fn test_sort_by_id() {
        let t1 = make_task(3, "medium", "todo");
        let t2 = make_task(1, "high", "todo");
        let t3 = make_task(2, "low", "todo");
        let mut tasks: Vec<&Task> = vec![&t1, &t2, &t3];

        let cfg = make_config();
        sort(&mut tasks, SortField::Id, false, &cfg);
        assert_eq!(tasks.iter().map(|t| t.id).collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn test_sort_by_id_reverse() {
        let t1 = make_task(1, "medium", "todo");
        let t2 = make_task(2, "high", "todo");
        let t3 = make_task(3, "low", "todo");
        let mut tasks: Vec<&Task> = vec![&t1, &t2, &t3];

        let cfg = make_config();
        sort(&mut tasks, SortField::Id, true, &cfg);
        assert_eq!(tasks.iter().map(|t| t.id).collect::<Vec<_>>(), vec![3, 2, 1]);
    }

    #[test]
    fn test_sort_due_none_last() {
        let mut t1 = make_task(1, "medium", "todo");
        t1.due = None;
        let mut t2 = make_task(2, "medium", "todo");
        t2.due = Some(
            chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        );
        let mut tasks: Vec<&Task> = vec![&t1, &t2];

        let cfg = make_config();
        sort(&mut tasks, SortField::Due, false, &cfg);
        // t2 (has due) should come first, t1 (None) last
        assert_eq!(tasks[0].id, 2);
        assert_eq!(tasks[1].id, 1);
    }

    #[test]
    fn test_sort_field_from_str() {
        assert_eq!("id".parse::<SortField>().unwrap(), SortField::Id);
        assert_eq!("status".parse::<SortField>().unwrap(), SortField::Status);
        assert_eq!("priority".parse::<SortField>().unwrap(), SortField::Priority);
        assert_eq!("created".parse::<SortField>().unwrap(), SortField::Created);
        assert_eq!("updated".parse::<SortField>().unwrap(), SortField::Updated);
        assert_eq!("due".parse::<SortField>().unwrap(), SortField::Due);
        assert!("invalid".parse::<SortField>().is_err());
    }

    #[test]
    fn test_sort_field_display() {
        assert_eq!(SortField::Id.to_string(), "id");
        assert_eq!(SortField::Status.to_string(), "status");
        assert_eq!(SortField::Priority.to_string(), "priority");
    }

    #[test]
    fn test_sort_field_default() {
        assert_eq!(SortField::default(), SortField::Id);
    }
}
