use std::sync::Arc;

use crate::boundary::ProjectBoundary;
use crate::file_tracker::FileTracker;
use crate::todo::TodoState;
use crate::ToolRegistry;
use lsp::LspManager;

/// Helper: register a tool only if its name appears in the allowed list.
fn maybe_register(
    registry: &mut ToolRegistry,
    tool: Arc<dyn crate::ToolExecutor>,
    allowed: Option<&[String]>,
) {
    if let Some(names) = allowed {
        if !names.iter().any(|n| n == tool.name()) {
            return;
        }
    }
    registry.register(tool);
}

struct RegisterFlags {
    include_agent_tool: bool,
    include_sub_agent_tool: bool,
    include_batch_tool: bool,
    include_ping_me_back_tools: bool,
    lsp_manager: Option<Arc<LspManager>>,
}

fn register_core_tools(registry: &mut ToolRegistry, flags: RegisterFlags) {
    let RegisterFlags {
        include_agent_tool,
        include_sub_agent_tool,
        include_batch_tool,
        include_ping_me_back_tools,
        lsp_manager,
    } = flags;

    let tracker = FileTracker::new();
    let boundary = ProjectBoundary::new(std::env::current_dir().unwrap_or_default());

    registry.register(Arc::new(
        crate::read::ReadTool::new()
            .with_file_tracker(tracker.clone())
            .with_boundary(boundary.clone()),
    ));
    registry.register(Arc::new(
        crate::glob::GlobTool::new().with_boundary(boundary.clone()),
    ));
    registry.register(Arc::new(crate::ls::LsTool::new()));
    registry.register(Arc::new(
        crate::ast_grep::AstGrepTool::new().with_boundary(boundary.clone()),
    ));
    registry.register(Arc::new(
        crate::rip_grep::RipGrepTool::new().with_boundary(boundary.clone()),
    ));

    registry.register(Arc::new(crate::bash::BashTool::new()));
    registry.register(Arc::new(
        crate::edit::EditTool::new()
            .with_file_tracker(tracker.clone())
            .with_boundary(boundary.clone())
            .with_lsp_manager_opt(lsp_manager.clone()),
    ));
    registry.register(Arc::new(
        crate::write::WriteTool::new()
            .with_file_tracker(tracker.clone())
            .with_boundary(boundary.clone())
            .with_lsp_manager_opt(lsp_manager.clone()),
    ));
    registry.register(Arc::new(
        crate::apply_patch::ApplyPatchTool::new().with_lsp_manager_opt(lsp_manager.clone()),
    ));
    registry.register(Arc::new(
        crate::multiedit::MultiEditTool::new()
            .with_file_tracker(tracker)
            .with_boundary(boundary)
            .with_lsp_manager_opt(lsp_manager.clone()),
    ));

    let web_fetch_tool = if let Ok(url) = std::env::var("BRIDGE_WEB_URL") {
        crate::web_fetch::WebFetchTool::with_fallback(url)
    } else {
        crate::web_fetch::WebFetchTool::with_defaults()
    };
    registry.register(Arc::new(web_fetch_tool));

    if let Ok(base_url) = std::env::var("BRIDGE_WEB_URL") {
        let spider = Arc::new(crate::spider_tools::SpiderClient::new(base_url));
        registry.register(Arc::new(crate::spider_tools::WebCrawlTool::new(
            spider.clone(),
        )));
        registry.register(Arc::new(crate::spider_tools::WebSearchTool::new(
            spider.clone(),
        )));
        registry.register(Arc::new(crate::spider_tools::WebGetLinksTool::new(
            spider.clone(),
        )));
        registry.register(Arc::new(crate::spider_tools::WebScreenshotTool::new(
            spider.clone(),
        )));
        registry.register(Arc::new(crate::spider_tools::WebTransformTool::new(spider)));
    }

    let todo_state = TodoState::new();
    registry.register(Arc::new(crate::todo::TodoWriteTool::with_state(
        todo_state.clone(),
    )));
    registry.register(Arc::new(crate::todo::TodoReadTool::with_state(todo_state)));

    if let Some(manager) = lsp_manager {
        registry.register(Arc::new(crate::lsp_tool::LspTool::new(manager)));
    }

    if include_ping_me_back_tools {
        let ping_state = crate::ping_me_back::PingState::new();
        registry.register(Arc::new(crate::ping_me_back::PingMeBackTool::new(
            ping_state.clone(),
        )));
        registry.register(Arc::new(crate::ping_me_back::CancelPingTool::new(
            ping_state,
        )));
    }

    if include_agent_tool {
        registry.register(Arc::new(crate::self_agent::AgentTool::new()));
    }

    if include_sub_agent_tool {
        registry.register(Arc::new(crate::agent::SubAgentTool::new()));
    }

    if include_batch_tool {
        let tool_snapshot = registry.snapshot();
        registry.register(Arc::new(crate::batch::BatchTool::new(tool_snapshot)));
    }
}

/// Register all built-in tools into the given registry.
/// Filesystem tools are always registered.
/// WebSearch is registered only when SEARCH_ENDPOINT env var is set.
/// If an `LspManager` is provided, the LSP tool is also registered.
pub fn register_builtin_tools(registry: &mut ToolRegistry) {
    register_builtin_tools_with_lsp(registry, None);
}

/// Register all built-in tools, optionally including the LSP tool.
pub fn register_builtin_tools_with_lsp(
    registry: &mut ToolRegistry,
    lsp_manager: Option<Arc<LspManager>>,
) {
    register_core_tools(
        registry,
        RegisterFlags {
            include_agent_tool: true,
            include_sub_agent_tool: true,
            include_batch_tool: true,
            include_ping_me_back_tools: true,
            lsp_manager,
        },
    );
}

/// Register built-in tools for subagents (excludes the agent tool).
///
/// Subagents are leaf-level workers and should not be able to spawn
/// other subagents. This prevents unbounded recursion.
pub fn register_builtin_tools_for_subagent(registry: &mut ToolRegistry) {
    register_core_tools(
        registry,
        RegisterFlags {
            include_agent_tool: false,
            include_sub_agent_tool: false,
            include_batch_tool: true,
            include_ping_me_back_tools: false,
            lsp_manager: None,
        },
    );
}

/// Register only the built-in tools whose names appear in `allowed_tools`.
///
/// If `allowed_tools` is empty, NO tools are registered — an empty list means
/// the agent intentionally has no built-in tools.
/// Unknown tool names in the list are silently ignored.
pub fn register_filtered_builtin_tools(registry: &mut ToolRegistry, allowed_tools: &[String]) {
    register_filtered_builtin_tools_with_lsp(registry, allowed_tools, None);
}

/// Register filtered built-in tools, optionally including the LSP tool.
pub fn register_filtered_builtin_tools_with_lsp(
    registry: &mut ToolRegistry,
    allowed_tools: &[String],
    lsp_manager: Option<Arc<LspManager>>,
) {
    if allowed_tools.is_empty() {
        return;
    }

    let filter = Some(allowed_tools);
    let tracker = FileTracker::new();
    let boundary = ProjectBoundary::new(std::env::current_dir().unwrap_or_default());

    // Filesystem search tools
    maybe_register(
        registry,
        Arc::new(
            crate::read::ReadTool::new()
                .with_file_tracker(tracker.clone())
                .with_boundary(boundary.clone()),
        ),
        filter,
    );
    maybe_register(
        registry,
        Arc::new(crate::glob::GlobTool::new().with_boundary(boundary.clone())),
        filter,
    );
    maybe_register(registry, Arc::new(crate::ls::LsTool::new()), filter);
    maybe_register(
        registry,
        Arc::new(crate::ast_grep::AstGrepTool::new().with_boundary(boundary.clone())),
        filter,
    );
    maybe_register(
        registry,
        Arc::new(crate::rip_grep::RipGrepTool::new().with_boundary(boundary.clone())),
        filter,
    );

    // Write-side tools (with LSP manager for diagnostics)
    maybe_register(registry, Arc::new(crate::bash::BashTool::new()), filter);
    maybe_register(
        registry,
        Arc::new(
            crate::edit::EditTool::new()
                .with_file_tracker(tracker.clone())
                .with_boundary(boundary.clone())
                .with_lsp_manager_opt(lsp_manager.clone()),
        ),
        filter,
    );
    maybe_register(
        registry,
        Arc::new(
            crate::write::WriteTool::new()
                .with_file_tracker(tracker.clone())
                .with_boundary(boundary.clone())
                .with_lsp_manager_opt(lsp_manager.clone()),
        ),
        filter,
    );
    maybe_register(
        registry,
        Arc::new(
            crate::apply_patch::ApplyPatchTool::new().with_lsp_manager_opt(lsp_manager.clone()),
        ),
        filter,
    );
    maybe_register(
        registry,
        Arc::new(
            crate::multiedit::MultiEditTool::new()
                .with_file_tracker(tracker)
                .with_boundary(boundary)
                .with_lsp_manager_opt(lsp_manager.clone()),
        ),
        filter,
    );

    // Web fetch (with optional fallback service)
    let web_fetch_filtered = if let Ok(url) = std::env::var("BRIDGE_WEB_URL") {
        crate::web_fetch::WebFetchTool::with_fallback(url)
    } else {
        crate::web_fetch::WebFetchTool::with_defaults()
    };
    maybe_register(registry, Arc::new(web_fetch_filtered), filter);

    // Spider-backed web tools (search, crawl, links, screenshot, transform)
    if let Ok(base_url) = std::env::var("BRIDGE_WEB_URL") {
        let spider = Arc::new(crate::spider_tools::SpiderClient::new(base_url));
        maybe_register(
            registry,
            Arc::new(crate::spider_tools::WebSearchTool::new(spider.clone())),
            filter,
        );
        maybe_register(
            registry,
            Arc::new(crate::spider_tools::WebCrawlTool::new(spider.clone())),
            filter,
        );
        maybe_register(
            registry,
            Arc::new(crate::spider_tools::WebGetLinksTool::new(spider.clone())),
            filter,
        );
        maybe_register(
            registry,
            Arc::new(crate::spider_tools::WebScreenshotTool::new(spider.clone())),
            filter,
        );
        maybe_register(
            registry,
            Arc::new(crate::spider_tools::WebTransformTool::new(spider)),
            filter,
        );
    }

    // Todo tools
    let todo_state = TodoState::new();
    maybe_register(
        registry,
        Arc::new(crate::todo::TodoWriteTool::with_state(todo_state.clone())),
        filter,
    );
    maybe_register(
        registry,
        Arc::new(crate::todo::TodoReadTool::with_state(todo_state)),
        filter,
    );

    // LSP tool — code intelligence (only if manager provided and allowed)
    if let Some(manager) = lsp_manager {
        maybe_register(
            registry,
            Arc::new(crate::lsp_tool::LspTool::new(manager)),
            filter,
        );
    }

    // Self-delegation agent tool
    maybe_register(
        registry,
        Arc::new(crate::self_agent::AgentTool::new()),
        filter,
    );

    // Sub-agent tool
    maybe_register(
        registry,
        Arc::new(crate::agent::SubAgentTool::new()),
        filter,
    );

    // Batch tool — registered last with a snapshot of all other tools
    if allowed_tools.iter().any(|n| n == "batch") {
        let tool_snapshot = registry.snapshot();
        registry.register(Arc::new(crate::batch::BatchTool::new(tool_snapshot)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_all_builtin_tools() {
        let mut registry = ToolRegistry::new();
        register_builtin_tools(&mut registry);
        // Should have at least the core tools registered
        assert!(registry.get("bash").is_some());
        assert!(registry.get("Read").is_some());
        assert!(registry.get("edit").is_some());
        assert!(registry.get("write").is_some());
        assert!(registry.get("RipGrep").is_some());
        assert!(registry.get("AstGrep").is_some());
        assert!(registry.get("Glob").is_some());
        assert!(registry.get("todowrite").is_some());
        assert!(registry.get("todoread").is_some());
        assert!(registry.get("batch").is_some());
    }

    #[test]
    fn test_filtered_empty_list_registers_nothing() {
        let mut registry = ToolRegistry::new();
        register_filtered_builtin_tools(&mut registry, &[]);
        assert!(registry.list().is_empty());
    }

    #[test]
    fn test_filtered_specific_tools() {
        let mut registry = ToolRegistry::new();
        let allowed = vec!["bash".to_string(), "Read".to_string()];
        register_filtered_builtin_tools(&mut registry, &allowed);
        assert!(registry.get("bash").is_some());
        assert!(registry.get("Read").is_some());
        assert!(registry.get("edit").is_none());
        assert!(registry.get("write").is_none());
    }

    #[test]
    fn test_filtered_unknown_names_ignored() {
        let mut registry = ToolRegistry::new();
        let allowed = vec!["bash".to_string(), "nonexistent_tool".to_string()];
        register_filtered_builtin_tools(&mut registry, &allowed);
        assert!(registry.get("bash").is_some());
        assert!(registry.get("nonexistent_tool").is_none());
        // Only bash should be registered
        assert_eq!(registry.list().len(), 1);
    }
}
