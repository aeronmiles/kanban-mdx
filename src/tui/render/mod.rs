//! TUI rendering -- board columns, cards, list view, dialogs, detail view,
//! create wizard, reader panel, and all overlay modals.
//!
//! Uses `crate::model::task::Task` for real data and `super::app` for view
//! state and task helper functions.

mod board;
mod chrome;
mod detail;
mod dialogs;
mod layout;
mod overlays;
mod pickers;

// Re-export build_detail_lines so app.rs can reach it via super::render::build_detail_lines.
pub(crate) use detail::build_detail_lines;

use super::app::{App, AppView};
use super::theme;
use ratatui::Frame;

use board::render_board;
use chrome::render_search_bar;
use detail::render_detail;
use dialogs::{render_create_dialog, render_delete_confirm, render_goto_dialog, render_move_dialog};
use overlays::{render_debug, render_help, render_search_help};
use pickers::{render_branch_picker, render_confirm_branch, render_context_picker};

pub fn render(app: &App, frame: &mut Frame) {
    theme::set_active(app.theme_kind);
    theme::set_adjustments(app.brightness, app.saturation);
    match app.view {
        AppView::Board => {
            render_board(app, frame);
        }
        AppView::Search => {
            render_board(app, frame);
            render_search_bar(app, frame, true);
        }
        AppView::Help => {
            render_board(app, frame);
            render_help(app, frame);
        }
        AppView::SearchHelp => {
            if app.search_help_return == AppView::Detail {
                render_detail(app, frame);
            } else {
                render_board(app, frame);
                render_search_bar(app, frame, false);
            }
            render_search_help(app, frame);
        }
        AppView::MoveTask => {
            render_board(app, frame);
            render_move_dialog(app, frame);
        }
        AppView::ConfirmDelete => {
            render_board(app, frame);
            render_delete_confirm(app, frame);
        }
        AppView::Detail => {
            render_detail(app, frame);
        }
        AppView::CreateTask => {
            render_board(app, frame);
            render_create_dialog(app, frame);
        }
        // SemanticSearch is no longer a separate view — ~ prefix is handled
        // inside the existing Search and Detail find modes.
        AppView::Debug => {
            render_board(app, frame);
            render_debug(app, frame);
        }
        AppView::BranchPicker => {
            render_board(app, frame);
            render_branch_picker(app, frame);
        }
        AppView::ContextPicker => {
            render_board(app, frame);
            render_context_picker(app, frame);
        }
        AppView::ConfirmBranch => {
            render_board(app, frame);
            render_confirm_branch(app, frame);
        }
    }

    // Goto overlay renders on top of any view.
    if app.goto_active {
        render_goto_dialog(app, frame);
    }
}
