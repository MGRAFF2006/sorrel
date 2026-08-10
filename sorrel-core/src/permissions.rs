use crate::Principal;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Component, Path},
};

/// Result type used by permission metadata helpers.
pub type PermissionResult<T> = Result<T, PermissionError>;

/// Errors returned while reading permission metadata.
#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    /// A stored object had an unknown visibility value.
    #[error("invalid visibility {actual:?}")]
    InvalidVisibility {
        /// Actual serialized visibility value.
        actual: String,
    },
}

/// Object-level visibility carried by lanes, stacks, and future collaboration objects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Visibility {
    /// Only the owner or explicitly assigned principals should see the object.
    Private,
    /// Metadata may be visible while sensitive content remains hidden.
    Redacted,
    /// Visible to the selected team or teams named by policy.
    Team,
    /// Visible to reviewers named by policy.
    Review,
    /// Visible to everyone with repository access.
    Public,
}

impl Visibility {
    /// Returns the protocol string for this visibility value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Redacted => "redacted",
            Self::Team => "team",
            Self::Review => "review",
            Self::Public => "public",
        }
    }

    fn from_protocol(value: &str) -> PermissionResult<Self> {
        match value {
            "private" => Ok(Self::Private),
            "redacted" => Ok(Self::Redacted),
            "team" => Ok(Self::Team),
            "review" => Ok(Self::Review),
            "public" => Ok(Self::Public),
            actual => Err(PermissionError::InvalidVisibility {
                actual: actual.to_owned(),
            }),
        }
    }
}

/// Reference to a policy object or policy document outside this first core model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyRef {
    /// Stable policy identifier.
    pub id: String,
}

impl PolicyRef {
    /// Builds a policy reference.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

/// Reference to a grant object or grant record outside this first core model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GrantRef {
    /// Stable grant identifier.
    pub id: String,
}

impl GrantRef {
    /// Builds a grant reference.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

/// Resource named by permission metadata.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ResourceRef {
    /// Resource type, for example `path`, `repo`, `change`, or `secret`.
    pub resource_type: String,
    /// Type-specific stable resource identifier.
    pub value: String,
}

impl ResourceRef {
    /// Builds a resource reference.
    #[must_use]
    pub fn new(resource_type: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            resource_type: resource_type.into(),
            value: value.into(),
        }
    }

    /// Builds a path resource reference from a snapshot-relative path.
    #[must_use]
    pub fn path(path: impl AsRef<Path>) -> Self {
        Self::new("path", path_to_protocol_string(path.as_ref()))
    }
}

/// Audit sink or hook that should observe activity for an object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditHook {
    /// Hook identifier.
    pub id: String,
    /// Audit events this hook wants to receive.
    pub events: Vec<String>,
}

impl AuditHook {
    /// Builds an audit hook reference.
    #[must_use]
    pub fn new(id: impl Into<String>, events: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            id: id.into(),
            events: events.into_iter().map(Into::into).collect(),
        }
    }
}

/// Permission metadata embedded into Lane and Stack objects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionMetadata {
    /// Principal that owns the object.
    pub owner: Principal,
    /// Object-level visibility state.
    pub visibility: Visibility,
    /// Policy references that govern this object.
    pub policy_refs: Vec<PolicyRef>,
    /// Grant references that can authorize access to this object or touched resources.
    pub grant_refs: Vec<GrantRef>,
    /// Resources touched by this object.
    pub touched_resources: Vec<ResourceRef>,
    /// Audit hooks that should observe actions against this object.
    pub audit_hooks: Vec<AuditHook>,
}

impl PermissionMetadata {
    /// Builds permission metadata with no policies, grants, touched resources, or audit hooks.
    #[must_use]
    pub fn new(owner: Principal, visibility: Visibility) -> Self {
        Self {
            owner,
            visibility,
            policy_refs: Vec::new(),
            grant_refs: Vec::new(),
            touched_resources: Vec::new(),
            audit_hooks: Vec::new(),
        }
    }

    /// Replaces touched resources with sorted, deduplicated resource refs.
    pub(crate) fn set_canonical_touched_resources(
        &mut self,
        resources: impl IntoIterator<Item = ResourceRef>,
    ) {
        self.touched_resources = resources
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
    }

    /// Returns true when one of this metadata's referenced grants allows the capability.
    #[must_use]
    pub fn is_authorized_by_grant(
        &self,
        principal: &Principal,
        resource: &ResourceRef,
        capability: &str,
        grants: &[Grant],
    ) -> bool {
        let grant_refs = self
            .grant_refs
            .iter()
            .map(|grant_ref| grant_ref.id.as_str())
            .collect::<BTreeSet<_>>();

        grants.iter().any(|grant| {
            grant_refs.contains(grant.id.as_str()) && grant.allows(principal, resource, capability)
        })
    }
}

/// Concrete grant record used by callers that want local authorization checks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Grant {
    /// Stable grant identifier.
    pub id: String,
    /// Principal receiving the grant.
    pub subject: Principal,
    /// Resource this grant applies to.
    pub resource: ResourceRef,
    /// Capability strings this grant permits, for example `path.write`.
    pub capabilities: Vec<String>,
}

impl Grant {
    /// Builds a concrete grant record.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        subject: Principal,
        resource: ResourceRef,
        capabilities: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            id: id.into(),
            subject,
            resource,
            capabilities: capabilities.into_iter().map(Into::into).collect(),
        }
    }

    /// Returns true when this grant permits `capability` for `principal` and `resource`.
    #[must_use]
    pub fn allows(&self, principal: &Principal, resource: &ResourceRef, capability: &str) -> bool {
        self.subject == *principal
            && self.resource == *resource
            && self
                .capabilities
                .iter()
                .any(|granted| granted == capability)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredPermissionMetadata {
    owner: StoredPrincipal,
    visibility: String,
    policy_refs: Vec<StoredRef>,
    grant_refs: Vec<StoredRef>,
    touched_resources: Vec<StoredResourceRef>,
    audit_hooks: Vec<StoredAuditHook>,
}

impl StoredPermissionMetadata {
    pub(crate) fn from_metadata(metadata: &PermissionMetadata) -> Self {
        Self {
            owner: StoredPrincipal::from_principal(&metadata.owner),
            visibility: metadata.visibility.as_str().to_owned(),
            policy_refs: metadata
                .policy_refs
                .iter()
                .map(|policy_ref| StoredRef {
                    id: policy_ref.id.clone(),
                })
                .collect(),
            grant_refs: metadata
                .grant_refs
                .iter()
                .map(|grant_ref| StoredRef {
                    id: grant_ref.id.clone(),
                })
                .collect(),
            touched_resources: metadata
                .touched_resources
                .iter()
                .map(StoredResourceRef::from_ref)
                .collect(),
            audit_hooks: metadata
                .audit_hooks
                .iter()
                .map(StoredAuditHook::from_hook)
                .collect(),
        }
    }

    pub(crate) fn into_metadata(self) -> PermissionResult<PermissionMetadata> {
        Ok(PermissionMetadata {
            owner: self.owner.into_principal(),
            visibility: Visibility::from_protocol(&self.visibility)?,
            policy_refs: self
                .policy_refs
                .into_iter()
                .map(|policy_ref| PolicyRef { id: policy_ref.id })
                .collect(),
            grant_refs: self
                .grant_refs
                .into_iter()
                .map(|grant_ref| GrantRef { id: grant_ref.id })
                .collect(),
            touched_resources: self
                .touched_resources
                .into_iter()
                .map(StoredResourceRef::into_ref)
                .collect(),
            audit_hooks: self
                .audit_hooks
                .into_iter()
                .map(StoredAuditHook::into_hook)
                .collect(),
        })
    }
}

#[derive(Serialize, Deserialize)]
struct StoredRef {
    id: String,
}

#[derive(Serialize, Deserialize)]
struct StoredResourceRef {
    #[serde(rename = "type")]
    resource_type: String,
    value: String,
}

impl StoredResourceRef {
    fn from_ref(resource_ref: &ResourceRef) -> Self {
        Self {
            resource_type: resource_ref.resource_type.clone(),
            value: resource_ref.value.clone(),
        }
    }

    fn into_ref(self) -> ResourceRef {
        ResourceRef {
            resource_type: self.resource_type,
            value: self.value,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAuditHook {
    id: String,
    events: Vec<String>,
}

impl StoredAuditHook {
    fn from_hook(hook: &AuditHook) -> Self {
        Self {
            id: hook.id.clone(),
            events: hook.events.clone(),
        }
    }

    fn into_hook(self) -> AuditHook {
        AuditHook {
            id: self.id,
            events: self.events,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct StoredPrincipal {
    #[serde(rename = "type")]
    principal_type: String,
    id: String,
    #[serde(rename = "displayName", skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
}

impl StoredPrincipal {
    fn from_principal(principal: &Principal) -> Self {
        Self {
            principal_type: principal.principal_type.clone(),
            id: principal.id.clone(),
            display_name: principal.display_name.clone(),
        }
    }

    fn into_principal(self) -> Principal {
        Principal {
            principal_type: self.principal_type,
            id: self.id,
            display_name: self.display_name,
        }
    }
}

fn path_to_protocol_string(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}
