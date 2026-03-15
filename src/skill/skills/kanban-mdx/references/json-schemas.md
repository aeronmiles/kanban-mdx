# kanban-mdx JSON Output Schemas

Reference for parsing `show --json` output and error responses.

## Task Object

Returned by: `show --json` (also by other commands when `--json` is passed).

```json
{
  "id": 1,
  "title": "Task title",
  "status": "in-progress",
  "priority": "high",
  "created": "2026-02-07T10:30:00Z",
  "updated": "2026-02-07T11:00:00Z",
  "started": "2026-02-07T10:35:00Z",
  "completed": "2026-02-07T12:00:00Z",
  "assignee": "alice",
  "tags": ["bug", "frontend"],
  "due": "2026-03-01",
  "estimate": "4h",
  "parent": 5,
  "depends_on": [3, 4],
  "blocked": true,
  "block_reason": "Waiting on API keys",
  "claimed_by": "agent-1",
  "claimed_at": "2026-02-07T10:40:00Z",
  "class": "feature",
  "branch": "feat/my-feature",
  "worktree": "/path/to/worktree",
  "body": "Markdown body text",
  "file": ".kbmdx/tasks/001-task-title.md"
}
```

Fields omitted when empty/null: started, completed, assignee, tags, due,
estimate, parent, depends_on, blocked, block_reason, claimed_by, claimed_at,
class, branch, worktree, body, file.

When `show --children` is used, the task object includes an additional field:

```json
{
  "children_summary": {
    "total": 5,
    "done": 3,
    "todo": 2
  }
}
```

The `children_summary` keys (besides `total`) are the board's status names,
with counts flattened via serde `#[serde(flatten)]`.

## Error Response

Returned on errors when `--json` is active:

```json
{
  "error": "task not found",
  "code": "TASK_NOT_FOUND",
  "details": {"id": 99}
}
```

Error codes: TASK_NOT_FOUND, BOARD_NOT_FOUND, BOARD_ALREADY_EXISTS,
INVALID_INPUT, INVALID_STATUS, INVALID_PRIORITY, INVALID_DATE,
INVALID_TASK_ID, WIP_LIMIT_EXCEEDED, DEPENDENCY_NOT_FOUND,
SELF_REFERENCE, NO_CHANGES, BOUNDARY_ERROR, STATUS_CONFLICT,
CONFIRMATION_REQUIRED, TASK_CLAIMED, INVALID_CLASS, CLASS_WIP_EXCEEDED,
CLAIM_REQUIRED, NOTHING_TO_PICK, INVALID_GROUP_BY, INTERNAL_ERROR.

Exit codes: 1 for user errors, 2 for internal errors.
