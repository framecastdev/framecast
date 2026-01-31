# Framecast API Ã¢â‚¬â€ Formal Specification v0.0.1-SNAPSHOT

**Version:** 0.4.2
**Date:** 2025-01-30
**Status:** Draft

---

## Overview

This is the formal specification for the Framecast API, a comprehensive system for managing users, teams, jobs, assets, and authentication. This document defines all data entities, operations, permissions, constraints, and system behaviors required for implementation.

---

## Changelog

### v0.4.0 Changes

- **4.4:** Added `revoked_at` field to Invitation entity
- **4.8:** Added SSE reconnection protocol to JobEvent
- **4.13:** Added SystemAsset entity
- **8.1:** Added `update_profile` operation
- **8.2:** Added `list_teams`, `list_members`, `update_team` operations
- **8.3:** Added `list_invitations`, `revoke_invitation`, `resend_invitation` operations
- **8.5:** Added `list_jobs`, `get_job` operations
- **8.7:** Added Asset Operations section
- **8.8:** Added System Asset Operations section
- **9.1:** Expanded permission matrix with new endpoints
- **9.4:** Added API Key Scopes for new endpoints
- **10.3:** Added Webhook Event Types documentation
- **13.4:** Added System Asset Catalog with full listing

### v0.4.1 Changes

- **4.7:** Added `failure_type` and `credits_refunded` fields to Job entity
- **8.2:** Added `create_team` operation
- **8.5:** Added `clone_job` operation
- **9.1:** Added `POST /v1/teams` and `POST /v1/jobs/:id/clone` endpoints
- **11.4:** Added Team Limits section
- **12.6:** Added Credit Refund Policy section
- **Appendix F:** Removed Team Creation and Credit Refunds from deferred

### v0.0.1-SNAPSHOT Changes (Spec Integrity Pass)

- **4.12:** Added `credits_refunded` field to Usage entity with `net_credits` derived field
- **6 (all):** Complete rewrite of Invariants to match entity definitions exactly
  - Fixed role values: `{owner, admin, member, viewer}` (was incorrectly `{owner, member}`)
  - Fixed Job status: `processing` (was incorrectly `running`), `canceled` (was `cancelled`)
  - Fixed ApiKey field names: `key_hash` (was `hashed_key`), `revoked_at` (was `is_active`)
  - Removed references to non-existent fields (`email_verified`, `team_id` on Job, etc.)
  - Added invariants for v4.1 features (team limits, credit refunds, failure_type)
  - Added invariants for Project, AssetFile, Webhook, WebhookDelivery, Usage, SystemAsset
- **8.9:** Added complete Webhook Operations (list, get, create, update, delete, rotate_secret, test, list_deliveries, retry_delivery)
- **8.10:** Added complete API Key Operations (list, get, create, update, revoke)
- **8.11:** Added Project Archive Operations (`archive_project`, `unarchive_project`)
- **8.12:** Added Endpoint Mapping Table (operation Ã¢â€ â€™ HTTP method Ã¢â€ â€™ endpoint)
- **9.2:** Added Archive/Unarchive permissions for projects (Owner/Admin only)
- **9.3:** Added `webhooks:read` and `webhooks:write` scopes

### v0.0.1-SNAPSHOT Changes (Entity & Operations Completeness)

- **4.2:** Added NOT NULL constraint to Team.name (`String!`, min 1)
- **4.7:** Added `updated_at` field and ON UPDATE trigger to Job entity
- **4.9:** Added `updated_at` field and ON UPDATE trigger to AssetFile entity
- **8.12:** Added `validate_spec` and `estimate_spec` operation definitions
- **12.6:** Added Credit Source Rules section (clarifies User vs Team credit pools)
- **13.2:** Clarified SystemAsset ID format vs URN format, fixed spec examples to use entity ID

---

## Table of Contents

1. [Notation](01_Notation.md) Ã¢â‚¬â€ Conventions, symbols, and formatting used throughout the specification
2. [User Model](02_User_Model.md) Ã¢â‚¬â€ User identity, authentication, and account management
3. [URN Scheme](03_URN_Scheme.md) Ã¢â‚¬â€ Uniform Resource Naming conventions for all entities
4. [Entity Definitions](04_Entities.md) Ã¢â‚¬â€ Complete definitions of all core entities and their fields
5. [Relationships & State Machines](05_Relationships_States.md) Ã¢â‚¬â€ Entity relationships and state transitions
6. [Invariants & Constraints](06_Invariants.md) Ã¢â‚¬â€ Validation rules and system constraints
7. [Operations](07_Operations.md) Ã¢â‚¬â€ All API operations and their specifications
8. [API Permissions](08_Permissions.md) Ã¢â‚¬â€ Authorization matrix and permission scopes
9. [Validation Rules](09_Validation.md) Ã¢â‚¬â€ Input validation and error handling
10. [Rate Limits](10_Rate_Limits.md) Ã¢â‚¬â€ Rate limiting policies and quotas
11. [Storage & Retention](11_Storage.md) Ã¢â‚¬â€ Data storage, archival, and retention policies
12. [System Assets](12_System_Assets.md) Ã¢â‚¬â€ System-provided asset definitions and catalogs
13. [ER Diagram](13_ER_Diagram.md) Ã¢â‚¬â€ Entity relationship diagram and visual architecture
14. [Appendices](14_Appendices.md) Ã¢â‚¬â€ Additional reference material and deferred items

---

## How to Use This Specification

- **For Implementation:** Begin with sections 1-4 to understand core concepts, then refer to sections 7-9 for API implementation details.
- **For Integration:** Review sections 2-3 (authentication and URN scheme) and section 8 (permissions).
- **For Validation:** Refer to sections 6 and 9 for constraint and validation requirements.
- **For Architecture:** Consult sections 5, 13, and the appendices for system design and relationships.

---

## Key Concepts

This specification defines the complete contract for the Framecast API system, including:

- **Entity Model:** User, Team, Job, Asset, Invitation, and SystemAsset entities with full field definitions
- **State Machines:** State transitions for Jobs, Invitations, and other entities with triggering conditions
- **Authorization:** Role-based and permission-based access control with granular scopes
- **Operations:** RESTful and event-based operations with full request/response specifications
- **Constraints:** Business rules, rate limits, storage policies, and system invariants
- **System Assets:** Pre-defined assets and asset catalogs managed by the system

---

## Status and Version History

- **v0.0.1-SNAPSHOT (Current): January 30, 2025 â€” Entity completeness: Job/AssetFile updated_at, Team.name constraint, validate_spec, Credit Source Rules
- **v0.4.2:** January 30, 2025 â€” Spec integrity pass: fixed invariants, added missing operations (webhooks, API keys, archive), endpoint mapping table
- **v0.4.1:** January 30, 2025 Ã¢â‚¬â€ Added team creation, job cloning, team limits, and credit refund policy
- **v0.4.0:** January 2025 Ã¢â‚¬â€ Major expansion with system assets, SSE events, and permission matrix
- **v0.3.0:** Prior release
- **Status:** Draft Ã¢â‚¬â€ Subject to revision before final release
