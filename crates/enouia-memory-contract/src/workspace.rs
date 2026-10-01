//! Windows workspace IPC v1 (ADR-MEM-44): the typed channel between the
//! workspace frontend and the embedded Core. Same envelope conventions as the
//! Memory IPC (`schemaVersion: 1`, camelCase, a discriminant), with commands
//! for the owner's trusted surface.
//!
//! The page sends logical IDs and single-use picker tokens only. A token
//! stands for a file or folder the owner chose in a native dialog opened by
//! the shell; the page never sees or sends a path.

use crate::error::{ContractError, MemoryError, MemoryErrorCode, Violation};
use crate::hash::Sha256Hex;
use crate::ids::{
    BranchId, CandidateId, CapsuleId, CommitId, DispatchId, ImportId, MemoryId, OperationId,
    RequestId, SessionId, SourceId,
};
use crate::json::{Revision, SchemaVersion};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const LIST_DEFAULT_LIMIT: u32 = 20;
pub const LIST_MAX_LIMIT: u32 = 100;
pub const TEXT_MAX_CHARS: usize = 20_000;
pub const QUERY_MAX_CHARS: usize = 2_000;
pub const EXCERPT_MAX_BYTES: u64 = 8 * 1024;
pub const REVIEW_MAX_DECISIONS: usize = 20;
pub const CREATE_VAULT_PHRASE: &str = "create new vault";

/// `tok_` + 32 lowercase hex: a picker token.
pub fn is_token(value: &str) -> bool {
    hex_suffix(value, "tok_")
}

/// `pln_` + 32 lowercase hex: a review plan held by the Core.
pub fn is_plan_id(value: &str) -> bool {
    hex_suffix(value, "pln_")
}

fn hex_suffix(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|rest| {
        rest.len() == 32 && rest.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Empty {}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RootArgs {
    pub root_token: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateArgs {
    pub root_token: String,
    pub confirm_phrase: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryListArgs {
    pub include_inactive: bool,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub cursor: Option<String>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchArgs {
    pub query: String,
    pub include_historical: bool,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub cursor: Option<String>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryArgs {
    pub memory_id: MemoryId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExcerptArgs {
    pub source_id: SourceId,
    pub source_revision: Revision,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub start_byte: Option<u64>,
    pub max_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageArgs {
    #[serde(deserialize_with = "crate::json::nullable")]
    pub cursor: Option<String>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub limit: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionAction {
    Accept,
    EditAccept,
    Reject,
    Merge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeKind {
    Candidate,
    Memory,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MergeTargetArg {
    pub kind: MergeKind,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionArg {
    pub candidate_id: CandidateId,
    pub revision: Revision,
    pub action: DecisionAction,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub edited_content: Option<String>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub merge_target: Option<MergeTargetArg>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewPlanArgs {
    pub decisions: Vec<DecisionArg>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanArgs {
    pub plan_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfirmArgs {
    pub plan_id: String,
    pub diff_hash: Sha256Hex,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForgetMode {
    Forget,
    Purge,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForgetArgs {
    pub memory_id: MemoryId,
    pub mode: ForgetMode,
    pub with_dependents: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeletePreviewArgs {
    pub memory_id: MemoryId,
    pub with_dependents: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RememberArgs {
    pub text: String,
    pub claim_key: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CorrectionArgs {
    pub memory_id: MemoryId,
    pub revision: Revision,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportTokenArgs {
    pub import_token: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportStartArgs {
    pub import_token: String,
    pub account_alias: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportResumeArgs {
    pub import_id: ImportId,
    pub account_alias: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionArgs {
    pub session_id: SessionId,
    pub branch_id: BranchId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AskArgs {
    pub session_id: SessionId,
    pub branch_id: BranchId,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckpointArgs {
    pub session_id: SessionId,
    pub branch_id: BranchId,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextPreviewArgs {
    pub query: String,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub session_id: Option<SessionId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub branch_id: Option<BranchId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapsuleArgs {
    pub capsule_id: CapsuleId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DispatchArgs {
    pub dispatch_id: DispatchId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DestinationArgs {
    pub destination_token: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportArgs {
    pub export_token: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationArgs {
    pub operation_id: OperationId,
}

/// Every command with its typed arguments.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", content = "arguments", rename_all = "snake_case")]
pub enum Command {
    WorkspaceStatus(Empty),
    VaultOpen(RootArgs),
    VaultCreate(CreateArgs),
    VaultLock(Empty),
    VaultUnlock(Empty),
    MemoryList(MemoryListArgs),
    MemorySearch(SearchArgs),
    MemoryRead(MemoryArgs),
    SourceExcerpt(ExcerptArgs),
    CandidateList(PageArgs),
    ReviewPlan(ReviewPlanArgs),
    ReviewDiscard(PlanArgs),
    ReviewConfirm(ConfirmArgs),
    ForgetPlan(ForgetArgs),
    DeletePreview(DeletePreviewArgs),
    Remember(RememberArgs),
    CorrectionPropose(CorrectionArgs),
    ImportPreview(ImportTokenArgs),
    ImportStart(ImportStartArgs),
    ImportResume(ImportResumeArgs),
    ImportList(Empty),
    SessionList(Empty),
    SessionDetail(SessionArgs),
    SessionNew(Empty),
    SessionAsk(AskArgs),
    SessionCheckpoint(CheckpointArgs),
    ContextPreview(ContextPreviewArgs),
    ContextInspect(CapsuleArgs),
    DispatchInspect(DispatchArgs),
    IndexRebuild(Empty),
    VaultVerify(Empty),
    BackupExport(DestinationArgs),
    RestorePreview(ExportArgs),
    OperationGet(OperationArgs),
    OperationCancel(OperationArgs),
    OperationList(Empty),
}

/// Command names, in schema order.
pub const COMMANDS: [&str; 36] = [
    "workspace_status",
    "vault_open",
    "vault_create",
    "vault_lock",
    "vault_unlock",
    "memory_list",
    "memory_search",
    "memory_read",
    "source_excerpt",
    "candidate_list",
    "review_plan",
    "review_discard",
    "review_confirm",
    "forget_plan",
    "delete_preview",
    "remember",
    "correction_propose",
    "import_preview",
    "import_start",
    "import_resume",
    "import_list",
    "session_list",
    "session_detail",
    "session_new",
    "session_ask",
    "session_checkpoint",
    "context_preview",
    "context_inspect",
    "dispatch_inspect",
    "index_rebuild",
    "vault_verify",
    "backup_export",
    "restore_preview",
    "operation_get",
    "operation_cancel",
    "operation_list",
];

/// Commands that can commit to the Vault and therefore carry an idempotency key.
pub fn is_write(command: &str) -> bool {
    matches!(
        command,
        "review_confirm"
            | "forget_plan"
            | "remember"
            | "correction_propose"
            | "import_start"
            | "import_resume"
            | "session_new"
            | "session_ask"
            | "session_checkpoint"
            | "context_preview"
    )
}

/// Long-running commands: they answer `operation_started` with an operation ID.
pub fn is_long_running(command: &str) -> bool {
    matches!(
        command,
        "import_start" | "import_resume" | "index_rebuild" | "vault_verify" | "backup_export"
    )
}

impl Command {
    pub fn name(&self) -> &'static str {
        let value = serde_json::to_value(self).expect("commands serialize");
        let name = value["command"].as_str().expect("tagged");
        COMMANDS
            .iter()
            .find(|c| **c == name)
            .copied()
            .expect("every command is listed")
    }

    /// The only success kind the command may return.
    pub fn success_kind(&self) -> ResponseKind {
        success_kind(self.name())
    }
}

pub fn success_kind(command: &str) -> ResponseKind {
    use ResponseKind as K;
    if is_long_running(command) {
        return K::OperationStarted;
    }
    match command {
        "workspace_status" | "vault_open" | "vault_create" | "vault_lock" | "vault_unlock" => {
            K::WorkspaceStatus
        }
        "memory_list" => K::MemoryPage,
        "memory_search" => K::MemorySearchResult,
        "memory_read" => K::MemoryDetail,
        "source_excerpt" => K::SourceExcerpt,
        "candidate_list" => K::CandidatePage,
        "review_plan" | "forget_plan" => K::ReviewPlan,
        "review_discard" => K::Ack,
        "review_confirm" => K::ReviewCommitted,
        "delete_preview" => K::DeleteImpact,
        "remember" | "correction_propose" => K::CandidateProposed,
        "import_preview" => K::ImportPreview,
        "import_list" => K::ImportList,
        "session_list" => K::SessionList,
        "session_detail" => K::SessionDetail,
        "session_new" => K::SessionCreated,
        "session_ask" => K::SessionTurn,
        "session_checkpoint" => K::CheckpointProposed,
        "context_preview" => K::ContextPreview,
        "context_inspect" => K::ContextInspection,
        "dispatch_inspect" => K::DispatchInspection,
        "restore_preview" => K::RestorePreview,
        "operation_get" | "operation_cancel" => K::OperationStatus,
        "operation_list" => K::OperationList,
        _ => K::MemoryError,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub schema_version: SchemaVersion,
    pub request_id: RequestId,
    pub command: String,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub idempotency_key: Option<String>,
    pub arguments: Value,
}

fn typed<T: DeserializeOwned>(value: Value) -> Result<T, ContractError> {
    serde_json::from_value(value).map_err(|e| ContractError::Shape(e.to_string()))
}

fn check_text(text: &str, max: usize, pointer: &str, out: &mut Vec<Violation>) {
    let chars = text.chars().count();
    if chars == 0 || chars > max {
        out.push(Violation::new("workspace.text_length", pointer));
    }
}

fn check_limit(limit: Option<u32>, out: &mut Vec<Violation>) {
    if limit.is_some_and(|l| l == 0 || l > LIST_MAX_LIMIT) {
        out.push(Violation::new("workspace.limit", "/arguments/limit"));
    }
}

fn check_cursor(cursor: Option<&String>, out: &mut Vec<Violation>) {
    if cursor.is_some_and(|c| c.is_empty() || c.len() > 512) {
        out.push(Violation::new("workspace.cursor", "/arguments/cursor"));
    }
}

fn check_token(token: &str, pointer: &str, out: &mut Vec<Violation>) {
    if !is_token(token) {
        out.push(Violation::new("workspace.token", pointer));
    }
}

fn check_alias(alias: &str, out: &mut Vec<Violation>) {
    if !crate::source::is_account_alias(alias) {
        out.push(Violation::new(
            "source.account_alias",
            "/arguments/accountAlias",
        ));
    }
}

/// Parse and validate a request envelope and its command's arguments.
pub fn parse_request(value: &Value) -> Result<(Request, Command), ContractError> {
    if !value.is_object() {
        return Err(ContractError::Malformed);
    }
    if value.get("schemaVersion").and_then(Value::as_i64) != Some(1) {
        return Err(ContractError::UnsupportedSchema {
            found: value.get("schemaVersion").and_then(Value::as_i64),
        });
    }
    let mut out = Vec::new();
    crate::json::check_safe_integers(value, "", &mut out);
    if !out.is_empty() {
        return Err(ContractError::Invalid(out));
    }
    let request: Request = typed(value.clone())?;
    if !COMMANDS.contains(&request.command.as_str()) {
        return Err(ContractError::Invalid(vec![Violation::new(
            "workspace.unknown_command",
            "/command",
        )]));
    }
    match (&request.idempotency_key, is_write(&request.command)) {
        (Some(key), true) if crate::ipc::is_idempotency_key(key) => {}
        (None, false) => {}
        _ => out.push(Violation::new("ipc.idempotency_key", "/idempotencyKey")),
    }
    let command: Command =
        typed(json!({"command": request.command, "arguments": request.arguments}))?;
    match &command {
        Command::VaultOpen(a) => check_token(&a.root_token, "/arguments/rootToken", &mut out),
        Command::VaultCreate(a) => {
            check_token(&a.root_token, "/arguments/rootToken", &mut out);
            if a.confirm_phrase != CREATE_VAULT_PHRASE {
                out.push(Violation::new(
                    "workspace.confirm_phrase",
                    "/arguments/confirmPhrase",
                ));
            }
        }
        Command::MemoryList(a) => {
            check_cursor(a.cursor.as_ref(), &mut out);
            check_limit(a.limit, &mut out);
        }
        Command::MemorySearch(a) => {
            check_text(&a.query, QUERY_MAX_CHARS, "/arguments/query", &mut out);
            check_cursor(a.cursor.as_ref(), &mut out);
            check_limit(a.limit, &mut out);
        }
        Command::SourceExcerpt(a) => {
            if a.max_bytes == 0 || a.max_bytes > EXCERPT_MAX_BYTES {
                out.push(Violation::new(
                    "ipc.source_max_bytes",
                    "/arguments/maxBytes",
                ));
            }
        }
        Command::CandidateList(a) => {
            check_cursor(a.cursor.as_ref(), &mut out);
            check_limit(a.limit, &mut out);
        }
        Command::ReviewPlan(a) => {
            if a.decisions.is_empty() || a.decisions.len() > REVIEW_MAX_DECISIONS {
                out.push(Violation::new(
                    "workspace.decision_count",
                    "/arguments/decisions",
                ));
            }
            for (i, d) in a.decisions.iter().enumerate() {
                let at = format!("/arguments/decisions/{i}");
                if (d.action == DecisionAction::EditAccept) != d.edited_content.is_some() {
                    out.push(Violation::new("ipc.edited_content", &at));
                }
                if let Some(text) = &d.edited_content {
                    check_text(text, TEXT_MAX_CHARS, &at, &mut out);
                }
                if (d.action == DecisionAction::Merge) != d.merge_target.is_some() {
                    out.push(Violation::new("review.merge_target", &at));
                }
                if let Some(target) = &d.merge_target {
                    let prefix = match target.kind {
                        MergeKind::Candidate => "cand",
                        MergeKind::Memory => "mem",
                    };
                    if crate::ids::check_prefixed_uuid(&target.id, prefix).is_err() {
                        out.push(Violation::new("ref.kind_prefix", &at));
                    }
                }
            }
        }
        Command::ReviewDiscard(a) => {
            if !is_plan_id(&a.plan_id) {
                out.push(Violation::new("workspace.plan_id", "/arguments/planId"));
            }
        }
        Command::ReviewConfirm(a) => {
            if !is_plan_id(&a.plan_id) {
                out.push(Violation::new("workspace.plan_id", "/arguments/planId"));
            }
        }
        Command::Remember(a) => {
            check_text(&a.text, TEXT_MAX_CHARS, "/arguments/text", &mut out);
            if !crate::memory::is_label(&a.claim_key) {
                out.push(Violation::new("workspace.claim_key", "/arguments/claimKey"));
            }
        }
        Command::CorrectionPropose(a) => {
            check_text(&a.text, TEXT_MAX_CHARS, "/arguments/text", &mut out)
        }
        Command::ImportPreview(a) => {
            check_token(&a.import_token, "/arguments/importToken", &mut out)
        }
        Command::ImportStart(a) => {
            check_token(&a.import_token, "/arguments/importToken", &mut out);
            check_alias(&a.account_alias, &mut out);
        }
        Command::ImportResume(a) => check_alias(&a.account_alias, &mut out),
        Command::SessionAsk(a) => check_text(&a.text, TEXT_MAX_CHARS, "/arguments/text", &mut out),
        Command::SessionCheckpoint(a) => {
            check_text(&a.summary, TEXT_MAX_CHARS, "/arguments/summary", &mut out)
        }
        Command::ContextPreview(a) => {
            check_text(&a.query, QUERY_MAX_CHARS, "/arguments/query", &mut out);
            if a.session_id.is_some() != a.branch_id.is_some() {
                out.push(Violation::new("workspace.session_pair", "/arguments"));
            }
        }
        Command::BackupExport(a) => check_token(
            &a.destination_token,
            "/arguments/destinationToken",
            &mut out,
        ),
        Command::RestorePreview(a) => {
            check_token(&a.export_token, "/arguments/exportToken", &mut out)
        }
        _ => {}
    }
    if out.is_empty() {
        Ok((request, command))
    } else {
        Err(ContractError::Invalid(out))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseKind {
    WorkspaceStatus,
    MemoryPage,
    MemorySearchResult,
    MemoryDetail,
    SourceExcerpt,
    CandidatePage,
    ReviewPlan,
    ReviewCommitted,
    CandidateProposed,
    DeleteImpact,
    ImportPreview,
    ImportList,
    SessionList,
    SessionDetail,
    SessionCreated,
    SessionTurn,
    CheckpointProposed,
    ContextPreview,
    ContextInspection,
    DispatchInspection,
    RestorePreview,
    OperationStarted,
    OperationStatus,
    OperationList,
    Ack,
    MemoryError,
}

/// An error as the page sees it: a contract code, whether a retry can help,
/// and stable rule identifiers. Never paths, OS messages, or record text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceError {
    pub code: MemoryErrorCode,
    pub retryable: bool,
    pub rules: Vec<String>,
}

impl WorkspaceError {
    pub fn new(code: MemoryErrorCode, rules: &[&str]) -> Self {
        Self {
            code,
            retryable: code.default_retryable(),
            rules: rules.iter().map(|r| (*r).to_owned()).collect(),
        }
    }

    pub fn from_memory(error: &MemoryError, rules: Vec<String>) -> Self {
        Self {
            code: error.code,
            retryable: error.retryable,
            rules,
        }
    }

    pub fn from_contract(error: &ContractError) -> Self {
        Self {
            code: error.memory_code(),
            retryable: false,
            rules: error.rules().iter().map(|r| (*r).to_owned()).collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Response {
    pub schema_version: SchemaVersion,
    pub request_id: RequestId,
    pub kind: ResponseKind,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub vault_commit_id: Option<CommitId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub operation_id: Option<OperationId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub result: Option<Value>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub error: Option<WorkspaceError>,
}

impl Response {
    pub fn ok(request_id: RequestId, kind: ResponseKind, result: Value) -> Self {
        Self {
            schema_version: SchemaVersion,
            request_id,
            kind,
            vault_commit_id: None,
            operation_id: None,
            result: Some(result),
            error: None,
        }
    }

    pub fn failed(request_id: RequestId, error: WorkspaceError) -> Self {
        Self {
            schema_version: SchemaVersion,
            request_id,
            kind: ResponseKind::MemoryError,
            vault_commit_id: None,
            operation_id: None,
            result: None,
            error: Some(error),
        }
    }
}

/// Validate a response envelope against the command that produced it.
pub fn validate_response(command: &str, value: &Value) -> Result<Response, ContractError> {
    let mut out = Vec::new();
    crate::json::check_safe_integers(value, "", &mut out);
    if !out.is_empty() {
        return Err(ContractError::Invalid(out));
    }
    let response: Response = typed(value.clone())?;
    let is_error = response.kind == ResponseKind::MemoryError;
    if is_error != response.error.is_some() || is_error == response.result.is_some() {
        out.push(Violation::new("ipc.result_or_error", "/"));
    }
    if response.result.as_ref().is_some_and(|r| !r.is_object()) {
        out.push(Violation::new("ipc.result_or_error", "/result"));
    }
    if !is_error && response.kind != success_kind(command) {
        out.push(Violation::new("ipc.kind_for_operation", "/kind"));
    }
    if !is_error && is_long_running(command) != response.operation_id.is_some() {
        out.push(Violation::new("workspace.operation_id", "/operationId"));
    }
    if response.error.as_ref().is_some_and(|e| {
        e.rules.len() > 16
            || e.rules.iter().any(|r| {
                r.is_empty()
                    || r.len() > 64
                    || !r
                        .bytes()
                        .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.'))
            })
    }) {
        out.push(Violation::new("workspace.error_rules", "/error/rules"));
    }
    if out.is_empty() {
        Ok(response)
    } else {
        Err(ContractError::Invalid(out))
    }
}
