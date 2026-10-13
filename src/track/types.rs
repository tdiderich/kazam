use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct TaskStore {
    pub tasks: Vec<Task>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Task {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub status: TaskStatus,
    #[serde(
        default = "default_priority",
        deserialize_with = "deserialize_priority"
    )]
    pub priority: u8,
    #[serde(default)]
    pub task_type: TaskType,
    #[serde(default, skip_serializing_if = "is_agent")]
    pub owner: TaskOwner,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default = "default_timestamp")]
    pub created: String,
    #[serde(default = "default_timestamp")]
    pub updated: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub close_reason: Option<String>,
    /// Unmodelled keys from hand-edited files; kept so load->save is lossless.
    #[serde(flatten)]
    pub extra: std::collections::BTreeMap<String, serde_yaml::Value>,
}

fn default_priority() -> u8 {
    2
}

/// Hand-written tasks may omit timestamps; epoch sorts them before everything else.
fn default_timestamp() -> String {
    "1970-01-01T00:00:00Z".to_string()
}

const PRIORITY_HELP: &str =
    "an integer 0-9, or one of: critical/urgent/p0, high/p1, medium/normal/p2, low/p3";

fn parse_priority_str(s: &str) -> Result<u8, String> {
    let t = s.trim().to_lowercase();
    match t.as_str() {
        "critical" | "urgent" | "p0" => return Ok(0),
        "high" | "p1" => return Ok(1),
        "medium" | "normal" | "p2" => return Ok(2),
        "low" | "p3" => return Ok(3),
        _ => {}
    }
    match t.parse::<u8>() {
        Ok(n) if n <= 9 => Ok(n),
        _ => Err(format!(
            "invalid priority \"{s}\", expected {PRIORITY_HELP}"
        )),
    }
}

/// Accepts an integer 0-9 or a string alias (case-insensitive); always stored as u8.
fn deserialize_priority<'de, D>(d: D) -> Result<u8, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Error, Unexpected, Visitor};
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = u8;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "{PRIORITY_HELP}")
        }
        fn visit_u64<E: Error>(self, v: u64) -> Result<u8, E> {
            if v <= 9 {
                Ok(v as u8)
            } else {
                Err(E::invalid_value(Unexpected::Unsigned(v), &self))
            }
        }
        fn visit_i64<E: Error>(self, v: i64) -> Result<u8, E> {
            u64::try_from(v)
                .map_err(|_| E::invalid_value(Unexpected::Signed(v), &self))
                .and_then(|v| self.visit_u64(v))
        }
        fn visit_str<E: Error>(self, v: &str) -> Result<u8, E> {
            parse_priority_str(v).map_err(E::custom)
        }
    }
    d.deserialize_any(V)
}

fn is_agent(o: &TaskOwner) -> bool {
    *o == TaskOwner::Agent
}

#[derive(Serialize, Deserialize, Clone, Copy, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    #[default]
    #[serde(alias = "todo")]
    Open,
    #[serde(alias = "in_progress")]
    Active,
    #[serde(alias = "done")]
    Closed,
    Blocked,
    #[serde(alias = "skipped")]
    Deferred,
}

impl TaskStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Active => "active",
            Self::Closed => "closed",
            Self::Blocked => "blocked",
            Self::Deferred => "deferred",
        }
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "open" => Ok(Self::Open),
            "active" => Ok(Self::Active),
            "closed" => Ok(Self::Closed),
            "blocked" => Ok(Self::Blocked),
            "deferred" => Ok(Self::Deferred),
            _ => Err(format!("unknown status: {s}")),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    #[default]
    Task,
    Bug,
    Feature,
    Epic,
}

#[derive(Serialize, Deserialize, Clone, Copy, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskOwner {
    #[default]
    Agent,
    Human,
}

impl TaskOwner {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Human => "human",
        }
    }
}

impl std::fmt::Display for TaskOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl std::str::FromStr for TaskOwner {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "agent" => Ok(Self::Agent),
            "human" => Ok(Self::Human),
            _ => Err(format!("unknown owner: {s} (expected: agent, human)")),
        }
    }
}

impl TaskType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Bug => "bug",
            Self::Feature => "feature",
            Self::Epic => "epic",
        }
    }
}

impl std::fmt::Display for TaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

// ── Log ──────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct LogStore {
    pub events: Vec<LogEntry>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LogEntry {
    pub date: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default)]
    pub severity: LogSeverity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum LogSeverity {
    Major,
    Minor,
    #[default]
    Info,
}

// ── Ready result (agent-first: includes full context) ──

#[derive(Serialize)]
pub struct ReadyTask {
    #[serde(flatten)]
    pub task: Task,
    pub parent_title: Option<String>,
    pub blocker_titles: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(p: &str) -> Result<Task, serde_yaml::Error> {
        serde_yaml::from_str(&format!(
            "id: a\ntitle: t\npriority: {p}\ncreated: x\nupdated: x\n"
        ))
    }

    #[test]
    fn priority_accepts_int_alias_and_numeric_string() {
        assert_eq!(parse("3").unwrap().priority, 3);
        assert_eq!(parse("high").unwrap().priority, 1);
        assert_eq!(parse("Medium").unwrap().priority, 2);
        assert_eq!(parse("URGENT").unwrap().priority, 0);
        assert_eq!(parse("p3").unwrap().priority, 3);
        assert_eq!(parse("\"2\"").unwrap().priority, 2);
    }

    #[test]
    fn timestamps_default_when_missing() {
        let t: Task = serde_yaml::from_str("id: a\ntitle: t\n").unwrap();
        assert_eq!(t.created, "1970-01-01T00:00:00Z");
        assert_eq!(t.updated, t.created);
    }

    #[test]
    fn status_aliases() {
        let st = |v: &str| -> TaskStatus {
            serde_yaml::from_str::<Task>(&format!("id: a\ntitle: t\nstatus: {v}\n"))
                .unwrap()
                .status
        };
        assert!(st("done") == TaskStatus::Closed);
        assert!(st("todo") == TaskStatus::Open);
        assert!(st("skipped") == TaskStatus::Deferred);
    }

    #[test]
    fn unknown_keys_round_trip() {
        let y = "id: a\ntitle: t\nref: PLAN.md\nblocked_reason: why\nskip_reason: nope\n";
        let t: Task = serde_yaml::from_str(y).unwrap();
        let out = serde_yaml::to_string(&t).unwrap();
        let t2: Task = serde_yaml::from_str(&out).unwrap();
        assert_eq!(
            t2.extra.get("ref").and_then(|v| v.as_str()),
            Some("PLAN.md")
        );
        assert_eq!(
            t2.extra.get("blocked_reason").and_then(|v| v.as_str()),
            Some("why")
        );
        assert_eq!(
            t2.extra.get("skip_reason").and_then(|v| v.as_str()),
            Some("nope")
        );
    }

    #[test]
    fn priority_rejects_invalid() {
        let e = parse("bogus").err().unwrap().to_string();
        assert!(e.contains("critical/urgent/p0"), "{e}");
        assert!(parse("10").is_err());
        assert!(parse("-1").is_err());
    }
}
