use crate::ai::agent_sdk::driver::AgentDriver;

pub(crate) async fn refresh_loop(
    task_id: String,
    role_arn: String,
    region: String,
    foreground: &warpui::ModelSpawner<AgentDriver>,
) {
    let _ = (task_id, role_arn, region, foreground);
    futures::future::pending().await
}
