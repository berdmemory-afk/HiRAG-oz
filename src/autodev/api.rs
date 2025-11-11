//! API endpoints for autonomous software development

use crate::autodev::schemas::{Task, CreateTaskRequest, TaskListResponse, TaskStatus};
use crate::autodev::Orchestrator;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};
use uuid::Uuid;

/// Shared state for autodev API
#[derive(Clone)]
pub struct AutodevState {
    pub orchestrator: Arc<Orchestrator>,
    pub tasks: Arc<RwLock<HashMap<Uuid, Task>>>,
}

/// Build autodev API routes
pub fn build_autodev_routes(orchestrator: Arc<Orchestrator>) -> Router {
    let state = AutodevState {
        orchestrator,
        tasks: Arc::new(RwLock::new(HashMap::new())),
    };
    
    Router::new()
        .route("/api/v1/autodev/tasks", post(create_task))
        .route("/api/v1/autodev/tasks", get(list_tasks))
        .route("/api/v1/autodev/tasks/:id", get(get_task))
        .route("/api/v1/autodev/tasks/:id/cancel", post(cancel_task))
        .with_state(state)
}

/// Create a new task
async fn create_task(
    State(state): State<AutodevState>,
    Json(request): Json<CreateTaskRequest>,
) -> Result<Json<Task>, (StatusCode, String)> {
    info!("Creating new task: {}", request.title);
    
    let task = Task {
        id: Uuid::new_v4(),
        title: request.title,
        description: request.description,
        repo: request.repo,
        base_branch: request.base_branch,
        risk_tier: request.risk_tier,
        constraints: request.constraints,
        acceptance: request.acceptance,
        metrics: request.metrics,
        status: TaskStatus::Pending,
        pr_url: None,
        error: None,
    };
    
    // Store task
    {
        let mut tasks = state.tasks.write().await;
        tasks.insert(task.id, task.clone());
    }
    
    // Spawn task execution in background
    let orchestrator = state.orchestrator.clone();
    let tasks_map = state.tasks.clone();
    let task_id = task.id;
    
    tokio::spawn(async move {
        match orchestrator.run_task(task).await {
            Ok(completed_task) => {
                let mut tasks = tasks_map.write().await;
                tasks.insert(task_id, completed_task);
            }
            Err(e) => {
                error!("Task {} failed: {}", task_id, e);
                let mut tasks = tasks_map.write().await;
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.status = TaskStatus::Failed;
                    task.error = Some(e.to_string());
                }
            }
        }
    });
    
    Ok(Json(task))
}

/// List all tasks
async fn list_tasks(
    State(state): State<AutodevState>,
) -> Result<Json<TaskListResponse>, (StatusCode, String)> {
    let tasks = state.tasks.read().await;
    let task_list: Vec<Task> = tasks.values().cloned().collect();
    let total = task_list.len();
    
    Ok(Json(TaskListResponse {
        tasks: task_list,
        total,
    }))
}

/// Get task by ID
async fn get_task(
    State(state): State<AutodevState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Task>, (StatusCode, String)> {
    let tasks = state.tasks.read().await;
    
    tasks
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Task {} not found", id)))
}

/// Cancel a task
async fn cancel_task(
    State(state): State<AutodevState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Task>, (StatusCode, String)> {
    let mut tasks = state.tasks.write().await;
    
    let task = tasks
        .get_mut(&id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Task {} not found", id)))?;
    
    if matches!(task.status, TaskStatus::Pending | TaskStatus::Planning | TaskStatus::Executing) {
        task.status = TaskStatus::Cancelled;
        info!("Cancelled task {}", id);
        Ok(Json(task.clone()))
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            format!("Cannot cancel task in status {:?}", task.status),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autodev::config::AutodevConfig;
    use crate::autodev::init_autodev;

    #[tokio::test]
    async fn test_build_routes() {
        let config = AutodevConfig::default();
        let orchestrator = init_autodev(config).await.unwrap();
        let router = build_autodev_routes(Arc::new(orchestrator));
        // Router should be created successfully
    }
}