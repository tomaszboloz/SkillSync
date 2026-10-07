//! A repository release is not necessarily a release of the installed skill.
use super::github::GitHubService;
use crate::models::skill::{ManagedItemType, SkillMetadata, SkillStatus};

pub(crate) async fn verify(mut skill: SkillMetadata) -> SkillMetadata {
    if skill.is_git_repo || skill.item_type != ManagedItemType::Skill || !skill.update_available {
        return skill;
    }
    let Some(remote) = skill.remote_url.as_deref() else {
        return skill;
    };
    let Some(version) = skill.latest_version.as_deref() else {
        return skill;
    };
    if GitHubService::fetch_raw_skill_md(remote, version, &skill.name)
        .await
        .is_none()
    {
        reject(&mut skill);
    }
    skill
}

fn reject(skill: &mut SkillMetadata) {
    let reason = format!(
        "Nie potwierdzono manifestu skilla {} w {} dla wersji {}. Wydanie repozytorium nie potwierdza aktualizacji tego skilla. Sprawdź źródło, tag i połączenie; SkillSync nie zmieni lokalnej wersji bez właściwego manifestu.",
        skill.name, skill.remote_url.as_deref().unwrap_or_default(),
        skill.latest_version.as_deref().unwrap_or_default()
    );
    skill.update_available = false;
    skill.update_compatibility = Some(reason.clone());
    skill.status = SkillStatus::Error(reason);
}
