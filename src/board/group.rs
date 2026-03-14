//! Grouping tasks by various fields for display.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::str::FromStr;

use serde::Serialize;

use crate::model::config::Config;
use crate::model::task::Task;

const CLASS_STANDARD: &str = "standard";

/// Enum of fields by which tasks can be grouped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupField {
    Assignee,
    Tag,
    Class,
    Priority,
    Status,
}

impl GroupField {
    /// Returns the list of valid group-by field names as strings.
    pub fn valid_values() -> &'static [&'static str] {
        &["assignee", "tag", "class", "priority", "status"]
    }
}

impl FromStr for GroupField {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "assignee" => Ok(GroupField::Assignee),
            "tag" => Ok(GroupField::Tag),
            "class" => Ok(GroupField::Class),
            "priority" => Ok(GroupField::Priority),
            "status" => Ok(GroupField::Status),
            _ => Err(format!(
                "invalid group-by field {:?}; valid fields: {}",
                s,
                GroupField::valid_values().join(", ")
            )),
        }
    }
}

impl fmt::Display for GroupField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            GroupField::Assignee => "assignee",
            GroupField::Tag => "tag",
            GroupField::Class => "class",
            GroupField::Priority => "priority",
            GroupField::Status => "status",
        };
        write!(f, "{}", s)
    }
}

/// Summary of a single status column within a group.
#[derive(Debug, Clone, Serialize)]
pub struct StatusSummary {
    pub status: String,
    pub count: i32,
    #[serde(skip_serializing_if = "is_zero")]
    pub wip_limit: i32,
}

use crate::util::serde_helpers::is_zero;

/// A single group in a grouped view.
#[derive(Debug, Clone, Serialize)]
pub struct GroupSummary {
    pub key: String,
    pub statuses: Vec<StatusSummary>,
    pub total: i32,
}

/// Collection of groups produced by `group_by_summary`.
#[derive(Debug, Clone, Serialize)]
pub struct GroupedSummary {
    pub groups: Vec<GroupSummary>,
}

/// Groups tasks by the given field into a simple ordered map.
///
/// Returns a `BTreeMap` of group label to tasks. This is the simplified API
/// for cases where just the grouping is needed without per-status summaries.
pub fn group_by<'a>(tasks: &[&'a Task], field: GroupField) -> BTreeMap<String, Vec<&'a Task>> {
    let mut groups: BTreeMap<String, Vec<&'a Task>> = BTreeMap::new();
    for task in tasks {
        let keys = extract_group_keys(task, field);
        for key in keys {
            groups.entry(key).or_default().push(task);
        }
    }
    groups
}

/// Groups tasks by the specified field and returns full summaries per group.
///
/// For `Tag`, a task with multiple tags appears in multiple groups.
/// Unassigned/untagged tasks get placeholder keys `"(unassigned)"` / `"(untagged)"`.
/// Groups are sorted: status/priority by config order, others alphabetically.
pub fn group_by_summary(tasks: &[Task], field: GroupField, cfg: &Config) -> GroupedSummary {
    let mut groups: HashMap<String, Vec<&Task>> = HashMap::new();

    for t in tasks {
        let keys = extract_group_keys(t, field);
        for key in keys {
            groups.entry(key).or_default().push(t);
        }
    }

    let sorted_keys = sort_group_keys(&groups, field, cfg);

    let result_groups = sorted_keys
        .into_iter()
        .map(|key| {
            let group_tasks = &groups[&key];
            let statuses = group_status_summary(group_tasks, cfg);
            GroupSummary {
                total: group_tasks.len() as i32,
                key,
                statuses,
            }
        })
        .collect();

    GroupedSummary {
        groups: result_groups,
    }
}

/// Returns the list of valid `--group-by` field names.
///
/// Deprecated: use `GroupField::valid_values()` instead.
pub fn valid_group_by_fields() -> Vec<&'static str> {
    GroupField::valid_values().to_vec()
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn extract_group_keys(t: &Task, field: GroupField) -> Vec<String> {
    match field {
        GroupField::Assignee => {
            if t.assignee.is_empty() {
                vec!["(unassigned)".to_string()]
            } else {
                vec![t.assignee.clone()]
            }
        }
        GroupField::Tag => {
            if t.tags.is_empty() {
                vec!["(untagged)".to_string()]
            } else {
                t.tags.clone()
            }
        }
        GroupField::Class => {
            let cls = if t.class.is_empty() {
                CLASS_STANDARD.to_string()
            } else {
                t.class.clone()
            };
            vec![cls]
        }
        GroupField::Priority => vec![t.priority.clone()],
        GroupField::Status => vec![t.status.clone()],
    }
}

fn sort_group_keys(
    groups: &HashMap<String, Vec<&Task>>,
    field: GroupField,
    cfg: &Config,
) -> Vec<String> {
    let mut keys: Vec<String> = groups.keys().cloned().collect();

    match field {
        GroupField::Status => {
            keys.sort_by(|a, b| cfg.status_index(a).cmp(&cfg.status_index(b)));
        }
        GroupField::Priority => {
            keys.sort_by(|a, b| cfg.priority_index(a).cmp(&cfg.priority_index(b)));
        }
        GroupField::Class => {
            keys.sort_by(|a, b| cfg.class_index(a).cmp(&cfg.class_index(b)));
        }
        _ => {
            keys.sort();
        }
    }

    keys
}

fn group_status_summary(tasks: &[&Task], cfg: &Config) -> Vec<StatusSummary> {
    let mut counts: HashMap<&str, i32> = HashMap::new();
    for t in tasks {
        *counts.entry(t.status.as_str()).or_insert(0) += 1;
    }

    cfg.status_names()
        .iter()
        .map(|s| StatusSummary {
            status: s.clone(),
            count: *counts.get(s.as_str()).unwrap_or(&0),
            wip_limit: cfg.wip_limit(s),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_group_by_fields() {
        let fields = GroupField::valid_values();
        assert_eq!(
            fields,
            &["assignee", "tag", "class", "priority", "status"]
        );
    }

    #[test]
    fn test_group_by_status() {
        let t1 = Task {
            id: 1,
            status: "todo".to_string(),
            ..Default::default()
        };
        let t2 = Task {
            id: 2,
            status: "done".to_string(),
            ..Default::default()
        };
        let t3 = Task {
            id: 3,
            status: "todo".to_string(),
            ..Default::default()
        };
        let tasks: Vec<&Task> = vec![&t1, &t2, &t3];
        let groups = group_by(&tasks, GroupField::Status);
        assert_eq!(groups.get("todo").unwrap().len(), 2);
        assert_eq!(groups.get("done").unwrap().len(), 1);
    }

    #[test]
    fn test_group_by_tag_multi_group() {
        let t1 = Task {
            id: 1,
            tags: vec!["a".to_string(), "b".to_string()],
            ..Default::default()
        };
        let tasks: Vec<&Task> = vec![&t1];
        let groups = group_by(&tasks, GroupField::Tag);
        assert_eq!(groups.get("a").unwrap().len(), 1);
        assert_eq!(groups.get("b").unwrap().len(), 1);
    }

    #[test]
    fn test_group_by_unassigned() {
        let t1 = Task {
            id: 1,
            ..Default::default()
        };
        let tasks: Vec<&Task> = vec![&t1];
        let groups = group_by(&tasks, GroupField::Assignee);
        assert!(groups.contains_key("(unassigned)"));
    }

    #[test]
    fn test_group_by_untagged() {
        let t1 = Task {
            id: 1,
            tags: vec![],
            ..Default::default()
        };
        let tasks: Vec<&Task> = vec![&t1];
        let groups = group_by(&tasks, GroupField::Tag);
        assert!(groups.contains_key("(untagged)"));
    }

    #[test]
    fn test_group_field_from_str() {
        assert_eq!("assignee".parse::<GroupField>().unwrap(), GroupField::Assignee);
        assert_eq!("tag".parse::<GroupField>().unwrap(), GroupField::Tag);
        assert_eq!("class".parse::<GroupField>().unwrap(), GroupField::Class);
        assert_eq!("priority".parse::<GroupField>().unwrap(), GroupField::Priority);
        assert_eq!("status".parse::<GroupField>().unwrap(), GroupField::Status);
        assert!("invalid".parse::<GroupField>().is_err());
    }

    #[test]
    fn test_group_field_display() {
        assert_eq!(GroupField::Assignee.to_string(), "assignee");
        assert_eq!(GroupField::Tag.to_string(), "tag");
        assert_eq!(GroupField::Status.to_string(), "status");
    }
}
