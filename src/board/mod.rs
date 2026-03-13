pub mod branch_context;
pub mod deps;
pub mod filter;
pub mod group;
pub mod list;
pub mod log;
pub mod metrics;
pub mod pick;
pub mod sort;
pub mod undo;

// Re-export primary types and functions for convenience.
pub use deps::{DepDirection, DepResult, DepsOutput};
pub use filter::{filter_unblocked, is_unclaimed, FilterOptions};
pub use group::{
    group_by, group_by_summary, valid_group_by_fields, GroupField, GroupSummary, GroupedSummary,
    StatusSummary,
};
pub use list::{list, ListOptions};
pub use log::{append_log, log_mutation, read_log, LogEntry, LogFilterOptions};
pub use metrics::{compute_flow_metrics, AgingItem, BoardMetrics, FlowMetrics};
pub use pick::{pick, PickOptions};
pub use sort::{sort, sort_owned, SortField};
pub use undo::{
    append_undo_only, clear_redo_journal, load_stack, peek_redo, peek_undo, pop_redo, pop_undo,
    push_redo, record_undo, redo_depth, restore_file_snapshots, restore_snapshot, save_stack,
    snapshot_file, snapshot_files, undo_depth, FileSnapshot, Snapshot, UndoEntry, UndoStack,
};
