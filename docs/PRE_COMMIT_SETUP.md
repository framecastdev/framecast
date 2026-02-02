# Pre-commit Hooks Setup Guide

Industry-standard pre-commit hooks for code quality, security, and consistency.

## Quick Setup

```bash
# Install pre-commit
pip install pre-commit

# Install the hooks
pre-commit install

# Install all hook types (pre-commit, pre-push, commit-msg)
pre-commit install --install-hooks --hook-type pre-commit
pre-commit install --hook-type pre-push
pre-commit install --hook-type commit-msg

# Test the setup
pre-commit run --all-files
```

## Hook Categories

### 🎨 **Code Quality & Formatting**

- **Rust**: `rustfmt` (formatting), `clippy` (linting)
- **Python**: `black` (formatting), `isort` (imports), `flake8` (linting), `ruff` (fast linting)
- **General**: Trailing whitespace, end-of-file, line endings

### 🔒 **Security Scanning**

- **Secret Detection**: `detect-secrets` (API keys, tokens)
- **Security Linting**: `bandit` (Python), `semgrep` (multi-language)
- **Credential Detection**: AWS keys, private keys, passwords

### 📊 **Infrastructure as Code**

- **Terraform/OpenTofu**: Formatting, validation, linting with `tflint`
- **YAML/JSON**: Syntax validation
- **SQL**: Formatting and linting with `sqlfluff`

### 📝 **Documentation**

- **Markdown**: Linting with `markdownlint`
- **Link Checking**: Validate external/internal links
- **API Spec**: Custom validation for consistency

### ✅ **Git & Project Standards**

- **Conventional Commits**: Enforce commit message format
- **Merge Conflicts**: Detect unresolved conflicts
- **Large Files**: Prevent accidental large file commits
- **Custom Validations**: Migration naming, TODO removal

## Configuration Files

| File | Purpose |
|------|---------|
| `.pre-commit-config.yaml` | Main pre-commit configuration |
| `.secrets.baseline` | Allowed secrets baseline |
| `.markdown-link-check.json` | Link checking configuration |
| `.tflint.hcl` | Terraform linting rules |
| `pyproject.toml` | Python tool configuration |

## Running Hooks

```bash
# Run all hooks on staged files
pre-commit run

# Run all hooks on all files
pre-commit run --all-files

# Run specific hook
pre-commit run rustfmt
pre-commit run black
pre-commit run detect-secrets

# Skip hooks temporarily (use sparingly)
git commit --no-verify -m "emergency fix"
SKIP=flake8 git commit -m "skip flake8"

# Update hooks to latest versions
pre-commit autoupdate
```

## Commit Message Format

We enforce [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

**Types**: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`, `perf`, `ci`, `build`, `revert`

**Examples**:

```bash
git commit -m "feat: add user authentication endpoint"
git commit -m "fix: resolve database connection timeout"
git commit -m "docs: update API documentation"
git commit -m "test: add integration tests for job processing"
```

## Custom Validations

### Migration File Naming

- **Pattern**: `YYYYMMDDHHMMSS_description.sql`
- **Example**: `20240130120000_add_user_table.sql`

### Environment Variables

- Detects hardcoded credentials and URLs
- Enforces environment variable usage
- Allows test/example patterns

### API Specification

- Validates internal links in docs/spec/
- Checks for TODO/FIXME in specifications
- Ensures consistent heading format

## IDE Integration

### VS Code

Install the `Pre-commit` extension and add to `.vscode/settings.json`:

```json
{
  "python.linting.enabled": true,
  "python.linting.flake8Enabled": true,
  "python.formatting.provider": "black",
  "[rust]": {
    "editor.formatOnSave": true
  },
  "rust-analyzer.checkOnSave.command": "clippy"
}
```

### vim/neovim

Add to your config:

```vim
autocmd BufWritePre *.rs :silent !rustfmt %
autocmd BufWritePre *.py :silent !black %
```

## Troubleshooting

### Common Issues

**Hook installation failed:**

```bash
# Update pre-commit
pip install --upgrade pre-commit

# Clear cache and reinstall
pre-commit clean
pre-commit install --install-hooks
```

**Secret detected:**

```bash
# Add to .secrets.baseline if it's a false positive
detect-secrets scan --baseline .secrets.baseline

# Or use inline ignore
password = "fake_password"  # pragma: allowlist secret
```

**Rust tools not found:**

```bash
# Ensure Rust tools are installed
rustup component add rustfmt clippy

# Update PATH or use just commands
export PATH="$HOME/.cargo/bin:$PATH"
```

**Python formatting conflicts:**

```bash
# Ensure consistent configuration
pip install black isort flake8 ruff

# Check pyproject.toml for consistent settings
```

### Performance Optimization

**Faster Python linting:**

```bash
# Use ruff instead of flake8 for faster linting
# Already configured in .pre-commit-config.yaml
```

**Skip slow hooks in development:**

```bash
# Skip security scans for quick commits
SKIP=bandit,semgrep git commit -m "quick fix"

# Run full checks before push
pre-commit run --all-files
```

## Integration with CI/CD

Pre-commit hooks also run in CI/CD:

```yaml
# .github/workflows/ci.yml
- name: Run pre-commit
  uses: pre-commit/action@v3.0.0
```

The same quality standards apply in both local development and CI/CD.

## Best Practices

1. **Run hooks before committing**: `pre-commit run`
2. **Keep hooks updated**: `pre-commit autoupdate` monthly
3. **Don't skip hooks**: Use `--no-verify` only for emergencies
4. **Fix issues don't suppress**: Address root causes rather than ignoring warnings
5. **Team consistency**: Ensure all team members use the same hook configuration

This setup ensures consistent code quality, security, and style across the entire Framecast codebase.
