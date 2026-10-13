use std::collections::{HashMap, HashSet};

use super::types::{ReadyTask, Task, TaskStatus};

pub fn ready(tasks: &[Task]) -> Vec<ReadyTask> {
    let by_id: HashMap<&str, &Task> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();

    // blockers[T] = set of task IDs that must close before T can start.
    // If task A has blocks: [B], that means A blocks B, so B depends on A.
    let mut blockers: HashMap<&str, Vec<&str>> = HashMap::new();
    for task in tasks {
        for blocked_id in &task.blocks {
            blockers
                .entry(blocked_id.as_str())
                .or_default()
                .push(task.id.as_str());
        }
    }

    let mut result: Vec<ReadyTask> = Vec::new();
    for task in tasks {
        if task.status != TaskStatus::Open {
            continue;
        }
        let all_blockers_closed = blockers
            .get(task.id.as_str())
            .map(|deps| {
                deps.iter().all(|dep_id| {
                    by_id
                        .get(dep_id)
                        .map(|d| d.status == TaskStatus::Closed)
                        .unwrap_or(true) // missing blocker = treat as done
                })
            })
            .unwrap_or(true);

        if all_blockers_closed {
            let parent_title = task
                .parent
                .as_deref()
                .and_then(|pid| by_id.get(pid))
                .map(|p| p.title.clone());

            let blocker_titles: Vec<String> = blockers
                .get(task.id.as_str())
                .map(|deps| {
                    deps.iter()
                        .filter_map(|dep_id| by_id.get(dep_id).map(|d| d.title.clone()))
                        .collect()
                })
                .unwrap_or_default();

            result.push(ReadyTask {
                task: task.clone(),
                parent_title,
                blocker_titles,
            });
        }
    }

    result.sort_by_key(|r| r.task.priority);
    result
}

pub fn blocked_by(task_id: &str, tasks: &[Task]) -> Vec<String> {
    tasks
        .iter()
        .filter(|t| t.blocks.iter().any(|b| b == task_id) && t.status != TaskStatus::Closed)
        .map(|t| t.id.clone())
        .collect()
}

pub fn has_cycle(tasks: &[Task]) -> Option<String> {
    let ids: HashSet<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
    let mut visited: HashSet<&str> = HashSet::new();
    let mut stack: HashSet<&str> = HashSet::new();

    // Build adjacency: A blocks B means edge A → B
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for task in tasks {
        for blocked in &task.blocks {
            if ids.contains(blocked.as_str()) {
                adj.entry(task.id.as_str())
                    .or_default()
                    .push(blocked.as_str());
            }
        }
    }

    fn dfs<'a>(
        node: &'a str,
        adj: &HashMap<&str, Vec<&'a str>>,
        visited: &mut HashSet<&'a str>,
        stack: &mut HashSet<&'a str>,
    ) -> bool {
        visited.insert(node);
        stack.insert(node);
        if let Some(neighbors) = adj.get(node) {
            for &neighbor in neighbors {
                if !visited.contains(neighbor) {
                    if dfs(neighbor, adj, visited, stack) {
                        return true;
                    }
                } else if stack.contains(neighbor) {
                    return true;
                }
            }
        }
        stack.remove(node);
        false
    }

    for task in tasks {
        if !visited.contains(task.id.as_str())
            && dfs(task.id.as_str(), &adj, &mut visited, &mut stack)
        {
            return Some(task.id.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::types::{TaskStatus, TaskType};

    fn task(id: &str, status: TaskStatus, blocks: Vec<&str>, priority: u8) -> Task {
        Task {
            id: id.to_string(),
            title: format!("Task {id}"),
            status,
            priority,
            task_type: TaskType::Task,
            owner: crate::track::types::TaskOwner::Agent,
            assignee: None,
            parent: None,
            blocks: blocks.into_iter().map(String::from).collect(),
            related: vec![],
            note: None,
            created: String::new(),
            updated: String::new(),
            closed: None,
            close_reason: None,
            extra: Default::default(),
        }
    }

    #[test]
    fn ready_returns_unblocked_open_tasks() {
        // a blocks b → b is blocked, only a is ready
        let tasks = vec![
            task("a", TaskStatus::Open, vec!["b"], 1),
            task("b", TaskStatus::Open, vec![], 0),
        ];
        let r = ready(&tasks);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].task.id, "a");
    }

    #[test]
    fn ready_unblocks_when_blocker_closed() {
        // a blocks b, but a is closed → b is now ready
        let tasks = vec![
            task("a", TaskStatus::Closed, vec!["b"], 1),
            task("b", TaskStatus::Open, vec![], 0),
        ];
        let r = ready(&tasks);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].task.id, "b");
    }

    #[test]
    fn ready_sorts_by_priority() {
        let tasks = vec![
            task("a", TaskStatus::Open, vec![], 3),
            task("b", TaskStatus::Open, vec![], 0),
            task("c", TaskStatus::Open, vec![], 1),
        ];
        let r = ready(&tasks);
        let ids: Vec<&str> = r.iter().map(|r| r.task.id.as_str()).collect();
        assert_eq!(ids, vec!["b", "c", "a"]);
    }

    #[test]
    fn cycle_detection() {
        let tasks = vec![
            task("a", TaskStatus::Open, vec!["b"], 0),
            task("b", TaskStatus::Open, vec!["a"], 0),
        ];
        assert!(has_cycle(&tasks).is_some());
    }

    #[test]
    fn no_cycle() {
        let tasks = vec![
            task("a", TaskStatus::Open, vec!["b"], 0),
            task("b", TaskStatus::Open, vec![], 0),
        ];
        assert!(has_cycle(&tasks).is_none());
    }
}
