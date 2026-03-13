//! Text chunking for embedding task content.
//!
//! Splits tasks into embeddable chunks by markdown headings (`##` and `###`).
//! Chunk 0 is always the title + tags preamble. Subsequent chunks correspond
//! to markdown sections. Tasks without headers produce a single body chunk.

use crate::model::task::Task;

/// A chunk of task content prepared for embedding.
#[derive(Debug, Clone)]
pub struct Chunk {
    /// Parent task ID.
    pub task_id: i32,
    /// Chunk index: 0 = preamble (title+tags), 1+ = body sections.
    pub index: usize,
    /// Section header (empty for preamble, e.g. "## Architecture" for sections).
    pub header: String,
    /// Start line in the task body (0 for preamble).
    pub line: usize,
    /// The chunk content for embedding.
    pub text: String,
}

impl Chunk {
    /// Returns the composite index ID for this chunk: "taskID:chunkIndex".
    pub fn chunk_id(&self) -> String {
        format!("{}:{}", self.task_id, self.index)
    }
}

/// Extracts the task ID and chunk index from a composite chunk ID.
/// Returns `None` if the ID is not a valid chunk ID.
pub fn parse_chunk_id(id: &str) -> Option<(i32, usize)> {
    let parts: Vec<&str> = id.splitn(2, ':').collect();
    if parts.len() != 2 {
        return None;
    }
    let task_id: i32 = parts[0].parse().ok()?;
    let chunk_idx: usize = parts[1].parse().ok()?;
    Some((task_id, chunk_idx))
}

/// Splits a task into embeddable chunks by `##` and `###` markdown headers.
///
/// Chunk 0 is always the title + tags preamble. Subsequent chunks correspond
/// to markdown sections. Tasks without headers produce a single body chunk.
pub fn chunk_task(task: &Task) -> Vec<Chunk> {
    // Chunk 0: title + tags preamble.
    let mut preamble = task.title.clone();
    if !task.tags.is_empty() {
        preamble.push('\n');
        preamble.push_str(&task.tags.join(", "));
    }

    let mut chunks = vec![Chunk {
        task_id: task.id,
        index: 0,
        header: String::new(),
        line: 0,
        text: preamble,
    }];

    let body = task.body.trim();
    if body.is_empty() {
        return chunks;
    }

    let lines: Vec<&str> = body.split('\n').collect();
    let sections = split_sections(&lines);

    for (i, sec) in sections.iter().enumerate() {
        chunks.push(Chunk {
            task_id: task.id,
            index: i + 1,
            header: sec.header.clone(),
            line: sec.line,
            text: sec.text.clone(),
        });
    }

    chunks
}

/// Produces the full text representation of a task for content hashing.
pub fn task_content(task: &Task) -> String {
    format!(
        "{}\n{}\n{}",
        task.title,
        task.body,
        task.tags.join(",")
    )
}

// ---------------------------------------------------------------------------
// Internal section parsing
// ---------------------------------------------------------------------------

/// A parsed markdown section.
struct Section {
    /// The heading line (e.g. "## Architecture"), empty for preamble text.
    header: String,
    /// 0-based line number in the body.
    line: usize,
    /// Full section text including the header.
    text: String,
}

/// Splits markdown lines into sections at `##` and `###` boundaries.
/// Text before the first heading becomes its own section with an empty header.
fn split_sections(lines: &[&str]) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    let mut current: Option<Section> = None;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if is_heading(trimmed) {
            // Flush previous section.
            if let Some(mut sec) = current.take() {
                sec.text = sec.text.trim().to_string();
                if !sec.text.is_empty() {
                    sections.push(sec);
                }
            }
            current = Some(Section {
                header: trimmed.to_string(),
                line: i,
                text: format!("{}\n", line),
            });
        } else if let Some(ref mut sec) = current {
            sec.text.push_str(line);
            sec.text.push('\n');
        } else {
            // Text before any heading.
            current = Some(Section {
                header: String::new(),
                line: i,
                text: format!("{}\n", line),
            });
        }
    }

    // Flush last section.
    if let Some(mut sec) = current.take() {
        sec.text = sec.text.trim().to_string();
        if !sec.text.is_empty() {
            sections.push(sec);
        }
    }

    sections
}

/// Returns true if the line is a `##` or `###` markdown heading.
fn is_heading(line: &str) -> bool {
    line.starts_with("## ") || line.starts_with("### ")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(id: i32, title: &str, body: &str, tags: Vec<&str>) -> Task {
        Task {
            id,
            title: title.to_string(),
            body: body.to_string(),
            tags: tags.into_iter().map(String::from).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn chunk_task_title_only() {
        let task = make_task(1, "Simple task", "", vec!["bug"]);
        let chunks = chunk_task(&task);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].index, 0);
        assert!(chunks[0].header.is_empty());
        assert_eq!(chunks[0].chunk_id(), "1:0");
        assert!(chunks[0].text.contains("Simple task"));
        assert!(chunks[0].text.contains("bug"));
    }

    #[test]
    fn chunk_task_with_sections() {
        let body = "Introduction text here.\n\n\
                     ## Architecture\n\n\
                     The system uses a layered design.\n\n\
                     ### Data Layer\n\n\
                     Database access patterns.\n\n\
                     ## Testing\n\n\
                     Unit and integration tests.";

        let task = make_task(42, "Design doc", body, vec!["design"]);
        let chunks = chunk_task(&task);

        // Expect: preamble, intro text, ## Architecture, ### Data Layer, ## Testing
        assert!(chunks.len() >= 4, "expected at least 4 chunks, got {}", chunks.len());

        // Chunk 0: preamble
        assert!(chunks[0].header.is_empty());

        let headers: Vec<&str> = chunks.iter().map(|c| c.header.as_str()).collect();
        assert!(headers.contains(&"## Architecture"), "missing ## Architecture: {:?}", headers);
        assert!(headers.contains(&"### Data Layer"), "missing ### Data Layer: {:?}", headers);
        assert!(headers.contains(&"## Testing"), "missing ## Testing: {:?}", headers);
    }

    #[test]
    fn chunk_task_no_headers() {
        let task = make_task(5, "Plain task", "Just some body text\nwith multiple lines.", vec![]);
        let chunks = chunk_task(&task);

        // Preamble + one body chunk
        assert_eq!(chunks.len(), 2);
        assert!(chunks[1].header.is_empty());
        assert!(chunks[1].text.contains("Just some body text"));
    }

    #[test]
    fn chunk_task_empty_body() {
        let task = make_task(3, "No body", "", vec![]);
        let chunks = chunk_task(&task);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn parse_chunk_id_valid() {
        assert_eq!(parse_chunk_id("42:0"), Some((42, 0)));
        assert_eq!(parse_chunk_id("42:3"), Some((42, 3)));
        assert_eq!(parse_chunk_id("1:1"), Some((1, 1)));
    }

    #[test]
    fn parse_chunk_id_invalid() {
        assert_eq!(parse_chunk_id("42"), None);
        assert_eq!(parse_chunk_id("abc:0"), None);
        assert_eq!(parse_chunk_id("42:abc"), None);
        assert_eq!(parse_chunk_id(""), None);
    }

    #[test]
    fn chunk_task_line_numbers() {
        let body = "Line 0 intro.\n\n\
                     ## Section A\n\n\
                     Content A.\n\n\
                     ## Section B\n\n\
                     Content B.";

        let task = make_task(1, "Test", body, vec![]);
        let chunks = chunk_task(&task);

        for c in &chunks {
            if c.index == 0 {
                continue; // preamble
            }
            if c.header == "## Section A" {
                assert_eq!(c.line, 2, "Section A line");
            }
            if c.header == "## Section B" {
                assert_eq!(c.line, 6, "Section B line");
            }
        }
    }

    #[test]
    fn task_content_with_tags() {
        let task = make_task(1, "Fix bug", "Description", vec!["urgent", "backend"]);
        let content = task_content(&task);
        assert_eq!(content, "Fix bug\nDescription\nurgent,backend");
    }

    #[test]
    fn task_content_no_tags() {
        let task = make_task(1, "Simple task", "No tags", vec![]);
        let content = task_content(&task);
        assert_eq!(content, "Simple task\nNo tags\n");
    }
}
