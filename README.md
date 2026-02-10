# Framecast

Storyboard-to-video generation API. Converts YAML/JSON specs into videos via ComfyUI.

## Quick Start

```bash
# Install tools (just, cargo-lambda, sqlx-cli, etc.)
just setup

# Start local services (PostgreSQL, LocalStack)
just dev

# Run migrations
just migrate

# Start the API server with hot-reload
just lambda-watch
```

The API will be available at `http://localhost:3000`.

## Tech Stack

| Layer | Technology |
|-------|------------|
| API | Rust + Axum on AWS Lambda |
| Database | PostgreSQL (Supabase) |
| Auth | JWT + API Keys |
| AI/LLM | Anthropic Claude |
| Storage | AWS S3 |
| IaC | OpenTofu |
| Task Runner | [Just](https://just.systems) |

## Project Layout

```
domains/           # Domain-driven vertical slices
  teams/           #   Users, Teams, Memberships, Invitations, ApiKeys
  artifacts/       #   Artifacts (storyboards, characters, media), SystemAssets
  conversations/   #   Conversations, Messages (LLM chat)
  projects/        #   Projects, AssetFiles (stub)
  jobs/            #   Jobs, JobEvents (stub)
  webhooks/        #   Webhooks, WebhookDeliveries (stub)
crates/            # Shared infrastructure
  app/             #   Composition root, Lambda + local binaries
  auth/            #   JWT/API key authentication, extractors
  llm/             #   LLM provider abstraction (Anthropic, mock)
  email/           #   AWS SES email service
  common/          #   Shared error types, URN parsing, pagination
migrations/        # SQLx database migrations
infra/opentofu/    # Infrastructure as Code
tests/             # Integration and E2E tests
```

## Commands

Run `just` to see all available commands. Key ones:

| Command | Description |
|---------|-------------|
| `just dev` | Start local development services |
| `just test` | Run all Rust tests |
| `just test-e2e` | Run Python E2E tests |
| `just migrate` | Run pending database migrations |
| `just lambda-build` | Build Lambda deployment artifact |
| `just deploy-local` | Deploy to LocalStack |
| `just check` | Run all quality checks |

## Architecture

See [CLAUDE.md](CLAUDE.md) for detailed architecture documentation, coding conventions, and development rules.
