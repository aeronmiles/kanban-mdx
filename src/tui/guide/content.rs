//! Guide topic definitions and compile-time embedded markdown pages.

/// A single guide topic with its metadata and embedded markdown body.
pub struct GuideTopic {
    pub title: &'static str,
    pub section: &'static str,
    pub body: &'static str,
}

/// All guide topics in display order.
pub fn topics() -> &'static [GuideTopic] {
    &TOPICS
}

/// Return topic indices matching a filter string (case-insensitive
/// substring match on title and section).
pub fn filtered_indices(filter: &str) -> Vec<usize> {
    if filter.is_empty() {
        return (0..TOPICS.len()).collect();
    }
    let q = filter.to_lowercase();
    TOPICS
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            t.title.to_lowercase().contains(&q) || t.section.to_lowercase().contains(&q)
        })
        .map(|(i, _)| i)
        .collect()
}

static TOPICS: [GuideTopic; 8] = [
    GuideTopic {
        title: "Board Basics",
        section: "Getting Started",
        body: include_str!("pages/board-basics.md"),
    },
    GuideTopic {
        title: "Navigation & Views",
        section: "Getting Started",
        body: include_str!("pages/navigation.md"),
    },
    GuideTopic {
        title: "Search & Filter Syntax",
        section: "Search & Filter",
        body: include_str!("pages/search-syntax.md"),
    },
    GuideTopic {
        title: "Task Lifecycle",
        section: "Task Management",
        body: include_str!("pages/task-lifecycle.md"),
    },
    GuideTopic {
        title: "Priorities & Classes of Service",
        section: "Task Management",
        body: include_str!("pages/priorities-classes.md"),
    },
    GuideTopic {
        title: "Dependencies & Blocking",
        section: "Task Management",
        body: include_str!("pages/dependencies.md"),
    },
    GuideTopic {
        title: "Branches & Worktrees",
        section: "Branches & Worktrees",
        body: include_str!("pages/branches-worktrees.md"),
    },
    GuideTopic {
        title: "Configuration",
        section: "Customization",
        body: include_str!("pages/configuration.md"),
    },
];
