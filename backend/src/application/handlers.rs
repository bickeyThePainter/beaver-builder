//! Op handlers -- thin layer that decodes Ops and delegates to the orchestrator.
//! In v1, the orchestrator handles Ops directly. This module exists as an
//! extension point for adding cross-cutting concerns (logging, metrics,
//! authorization) without polluting the orchestrator.

// Currently a placeholder. As the system grows, each Op variant gets
// a dedicated handler function here that the orchestrator dispatches to.
