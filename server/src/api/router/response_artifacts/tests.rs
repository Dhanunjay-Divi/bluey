use super::*;

#[test]
fn recovers_presentation_sections_accidentally_left_inside_python_fence() {
    let answer = r#"To implement an LRU cache, combine a hash map with a doubly linked list.

```python
class Node:
    def __init__(self, key=0, value=0):
        self.key = key
        self.value = self.value  # Redundant assignment for clarity
        self.prev = None
        self.next = None

class LRUCache:
    def __init__(self, capacity: int):
        self.capacity = capacity
        self.cache = {}

    def get(self, key: int) -> int:
        node = self.cache.get(key)
        return -1 if node is None else node.value

Line notes:
1: Node stores the key, value, and list links.
2: The dictionary maps each key to its node.

Explanation
The dictionary provides constant-time lookup while the list preserves recency order.
- The dictionary provides constant-time lookup.
- The list preserves recency order.

Complexity
- Time Complexity: O(1) for get and put.
- Space Complexity: O(capacity).

Edge cases
- A missing key returns -1.
- Capacity zero stores nothing.
```"#;

    let artifact = response_artifact(answer).expect("code artifact");
    assert_eq!(artifact.artifact_type, "code");

    let code = extract_code_section_from_canvas(&artifact.body);
    assert!(code.contains("self.value = self.value  # Redundant assignment for clarity"));
    assert!(!code.contains("self.value = value"));
    assert!(code.contains("return -1 if node is None else node.value"));
    assert!(!code.contains("Line notes"));
    assert!(!code.contains("Explanation"));
    assert!(!code.contains("Time Complexity"));
    assert!(!code.contains("Edge cases"));

    assert!(artifact.body.contains("LINE NOTES\n----------"));
    assert!(artifact
        .body
        .contains("1: Node stores the key, value, and list links."));
    assert!(artifact.body.contains("COMPLEXITY\n----------"));
    assert!(artifact
        .body
        .contains("Time Complexity: O(1) for get and put."));
    assert!(artifact.body.contains("NOTES\n-----"));
    assert!(artifact.body.contains("Explanation"));
    assert!(artifact.body.contains(
        "The dictionary provides constant-time lookup while the list preserves recency order."
    ));
    assert!(artifact.body.contains("Edge cases"));
}

#[test]
fn recovers_slash_comment_presentation_headings() {
    let answer = r#"```javascript
function getValue(items) {
  return items[0];
}
// Explanation:
// - Return the first item.
// Complexity:
// - Time Complexity: O(1).
// Edge cases:
// - Empty input returns undefined.
```"#;

    let artifact = response_artifact(answer).expect("code artifact");
    let code = extract_code_section_from_canvas(&artifact.body);
    assert!(code.contains("return items[0];"));
    assert!(!code.contains("Explanation"));
    assert!(artifact.body.contains("COMPLEXITY\n----------"));
    assert!(artifact.body.contains("Edge cases:"));
}

#[test]
fn preserves_heading_like_comments_when_executable_code_follows() {
    let answer = r#"```python
def describe(items):
    # Explanation
    explanation = "items are counted once"
    # Complexity
    complexity = len(items)
    return explanation, complexity
```"#;

    let artifact = response_artifact(answer).expect("code artifact");
    let code = extract_code_section_from_canvas(&artifact.body);
    assert!(code.contains("# Explanation"));
    assert!(code.contains("explanation = \"items are counted once\""));
    assert!(code.contains("# Complexity"));
    assert!(code.contains("complexity = len(items)"));
    assert!(!artifact.body.contains("COMPLEXITY\n----------"));
}

#[test]
fn preserves_unrecognized_control_flow_after_multiple_heading_like_comments() {
    let answer = r#"```python
def require_value(value):
    if value is not None:
        return value
    # Explanation
    # Complexity
    else:
        raise ValueError("missing")
```"#;

    let artifact = response_artifact(answer).expect("code artifact");
    let code = extract_code_section_from_canvas(&artifact.body);
    assert!(code.contains("# Explanation"));
    assert!(code.contains("# Complexity"));
    assert!(code.contains("else:"));
    assert!(code.contains("raise ValueError"));
    assert!(!artifact.body.contains("COMPLEXITY\n----------"));
}

#[test]
fn preserves_a_single_heading_like_trailing_comment() {
    let answer = r#"```python
def identity(value):
    return value

# Explanation
```"#;

    let artifact = response_artifact(answer).expect("code artifact");
    let code = extract_code_section_from_canvas(&artifact.body);
    assert!(code.contains("# Explanation"));
    assert!(!artifact.body.contains("NOTES\n-----"));
}
