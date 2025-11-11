# Autonomous Software Development System

## Overview

This system implements a complete autonomous software development pipeline capable of:
- **Planning**: Analyzing tasks and generating execution plans
- **Coding**: Generating code changes using LLM
- **Testing**: Running builds and tests in sandboxed environments
- **Verification**: Static analysis, secrets scanning, policy checks
- **PR Management**: Creating pull requests and managing the review cycle

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     API Layer                                │
│  POST /api/v1/autodev/tasks                                 │
│  GET  /api/v1/autodev/tasks                                 │
│  GET  /api/v1/autodev/tasks/{id}                            │
│  POST /api/v1/autodev/tasks/{id}/cancel                     │
└────────────────────┬────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────┐
│                   Orchestrator                               │
│  - Task lifecycle management                                 │
│  - Plan generation                                           │
│  - Step execution with retries                               │
│  - Workspace management                                      │
└────────────────────┬────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────┐
│                    Tool Layer                                │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │   Git    │  │  Runner  │  │ Codegen  │  │  Policy  │   │
│  │  Tools   │  │  Tools   │  │   Tool   │  │  Tools   │   │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘   │
│  ┌──────────┐  ┌──────────┐                                 │
│  │  Search  │  │  Static  │                                 │
│  │  Tools   │  │ Analysis │                                 │
│  └──────────┘  └──────────┘                                 │
└─────────────────────────────────────────────────────────────┘
```

## Quick Start

### 1. Configuration

Add to your `config.toml`:

```toml
[autodev]
enabled = true
provider = "github"
max_parallel_tasks = 4
max_step_retries = 2
default_risk_tier = "low"
sandbox_image = "rust:1.82"
runner_timeout_secs = 1200
opa_url = "http://localhost:8181"
policy_package = "autodev/merge"
allowlist_repos = ["github.com/yourorg/*"]

[autodev.llm]
provider = "openai"
model = "gpt-4"
api_key_env = "OPENAI_API_KEY"
api_url = "https://api.openai.com/v1/chat/completions"
max_tokens = 4096
temperature = 0.2

[autodev.git]
github_token_env = "GITHUB_TOKEN"
git_author_name = "AutoDev Bot"
git_author_email = "autodev@yourorg.com"
```

### 2. Environment Variables

```bash
export AUTODEV_ENABLED=true
export OPENAI_API_KEY=your-api-key
export GITHUB_TOKEN=your-github-token
export OPA_URL=http://localhost:8181
export AUTODEV_ALLOWED_REPOS=github.com/yourorg/*
```

### 3. Start the Server

```bash
cargo run --bin context-manager
```

### 4. Submit a Task

```bash
curl -X POST http://localhost:8080/api/v1/autodev/tasks \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Fix flaky test in vision decode path",
    "description": "Tests intermittently time out for batch decode > 32 regions. Need to investigate and fix the timeout handling.",
    "repo": "https://github.com/yourorg/yourrepo.git",
    "base_branch": "main",
    "risk_tier": "low",
    "constraints": [
      "Do not modify public API signatures",
      "No new external dependencies"
    ],
    "acceptance": [
      "cargo test passes",
      "clippy warnings = 0",
      "no policy violations"
    ]
  }'
```

### 5. Check Task Status

```bash
curl http://localhost:8080/api/v1/autodev/tasks/{task-id}
```

## Tools Available

### Git Tools
- **git_clone**: Clone repository
- **git_apply**: Create branch, apply patch, commit
- **git_pr**: Create GitHub pull request

### Runner Tools
- **runner**: Execute arbitrary commands in Docker sandbox
- **build**: Build the project (cargo build)
- **test**: Run tests (cargo test)

### Code Generation
- **codegen**: Generate code changes using LLM

### Policy & Verification
- **policy**: Check policy using OPA
- **policy_local**: Local policy rules (fallback)
- **clippy**: Run Rust static analysis
- **secrets_scan**: Scan for secrets and credentials
- **check_deps**: Check dependencies

### Search & Analysis
- **repo_search**: Search code using ripgrep
- **file_list**: List repository files

## Risk Tiers

### Low Risk (Auto-merge)
- Small bug fixes
- Documentation updates
- Test improvements
- **Policy**: Auto-merge if tests pass and no warnings

### Medium Risk (Human Review)
- Feature additions
- Refactoring
- Dependency updates
- **Policy**: PR created, requires human review

### High Risk (Approvers Required)
- API changes
- Database migrations
- Security-sensitive code
- **Policy**: PR created, requires approvers + policy override

## Policy Rules

Default local policy enforces:
- ✅ Tests must pass
- ✅ No clippy warnings
- ✅ No secrets detected
- ✅ High-risk changes require human review
- ✅ Database schema changes require DBA approval
- ✅ No new dependencies without approval

### Custom OPA Policy

Create `policy.rego`:

```rego
package autodev.merge

default allow = false

deny[msg] {
  input.risk_tier == "high"
  msg := "High-risk changes require human review"
}

deny[msg] {
  some f in input.files_changed
  endswith(f, ".sql")
  msg := "DB schema changes require DBA approval"
}

deny[msg] {
  input.secrets_found
  msg := "Secrets detected in changes"
}

allow {
  input.clippy_warnings == 0
  input.tests_passed == true
  count(input.new_dependencies) == 0
  not deny[_]
}
```

## Metrics

Prometheus metrics available at `/metrics`:

```promql
# Task metrics
autodev_tasks_total
autodev_tasks_success_total
autodev_tasks_failed_total
autodev_task_duration_seconds

# Step metrics
autodev_steps_total{status="success|error"}
autodev_step_duration_seconds{tool="..."}

# PR metrics
autodev_prs_opened_total
autodev_merges_total
autodev_reverts_total

# Policy metrics
autodev_policy_denials_total{reason="..."}
```

## Safety & Governance

### Opt-Out Controls
- **Global**: `AUTODEV_ENABLED=false`
- **Per-repo**: Configure allowlist
- **Per-task**: Set risk tier to "high"

### Isolation
- Each task runs in isolated workspace
- Docker sandbox for build/test
- Network isolation (--network none)

### Audit Trail
- All tasks logged with full context
- Step-by-step execution recorded
- Policy decisions tracked
- PR links preserved

### Secrets Protection
- Automatic secrets scanning
- API keys redacted in logs
- No code exfiltration beyond configured providers

## Troubleshooting

### Task Stuck in "Executing"
- Check Docker daemon is running
- Verify sandbox image is available
- Check runner timeout settings

### Policy Denials
- Review policy rules in OPA
- Check clippy warnings: `cargo clippy`
- Verify tests pass: `cargo test`
- Scan for secrets: `gitleaks detect`

### LLM Errors
- Verify API key is set
- Check API endpoint is reachable
- Review token limits and quotas
- Check model availability

### Git Errors
- Verify GitHub token has correct permissions
- Check repository URL is accessible
- Ensure branch doesn't already exist

## Development

### Adding New Tools

1. Create tool implementation:

```rust
use crate::autodev::tools::{Tool, ToolContext, ToolError};
use async_trait::async_trait;

pub struct MyTool;

#[async_trait]
impl Tool for MyTool {
    fn name(&self) -> &'static str {
        "my_tool"
    }
    
    fn description(&self) -> &'static str {
        "My custom tool"
    }
    
    async fn invoke(&self, input: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        // Implementation
        Ok(serde_json::json!({"result": "success"}))
    }
}
```

2. Register in `src/autodev/mod.rs`:

```rust
tools.push(Arc::new(MyTool));
```

### Running Tests

```bash
cargo test --package context-manager --lib autodev
```

### Building Documentation

```bash
cargo doc --no-deps --open
```

## Roadmap

### Phase 1 (Current)
- ✅ Core orchestration
- ✅ Git operations
- ✅ Docker sandbox
- ✅ LLM code generation
- ✅ Policy enforcement
- ✅ Static analysis

### Phase 2 (Next)
- [ ] Advanced planning with LLM
- [ ] Multi-language support (Python, Go, TypeScript)
- [ ] Code review integration
- [ ] Automated dependency updates

### Phase 3 (Future)
- [ ] Multi-repo coordination
- [ ] Performance regression detection
- [ ] Cost optimization
- [ ] Advanced metrics and dashboards

## License

MIT

## Support

For issues and questions:
- GitHub Issues: https://github.com/yourorg/yourrepo/issues
- Documentation: https://docs.yourorg.com/autodev
- Slack: #autodev-support