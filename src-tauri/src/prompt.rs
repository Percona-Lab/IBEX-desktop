//! System prompt generation — Rust port of scripts/build-prompt.sh.
//!
//! Builds the system prompt dynamically based on which connectors
//! are configured in IbexConfig.

use crate::config::IbexConfig;

/// Build the system prompt based on configured connectors.
/// Output matches the bash build_system_prompt() function.
pub fn build_system_prompt(config: &IbexConfig) -> String {
    let mut prompt = String::from("You have access to workplace tools via IBEX:\n");

    if config.is_slack_configured() {
        prompt.push_str("\n- Slack: search messages, read channels, list channels, read threads");
    }

    if config.is_notion_configured() {
        prompt.push_str("\n- Notion: search pages, read content, query databases");
    }

    if config.is_jira_configured() {
        prompt.push_str(
            "\n- Jira: search issues with JQL, read issue details and comments, list projects",
        );
    }

    if config.is_servicenow_configured() {
        prompt.push_str("\n- ServiceNow: query tables, get records, list tables");
    }

    if config.is_salesforce_configured() {
        prompt.push_str(
            "\n- Salesforce: run SOQL queries, get records, search across objects, describe schemas",
        );
    }

    if config.is_memory_configured() {
        prompt.push_str("\n- Memory: read/write persistent memory (GitHub-backed)");
        prompt.push_str("\n");
        prompt.push_str("\nMemory usage:");
        prompt.push_str("\n- Use memory_get when the user references previous context, asks \"what do you know\", or needs background on a project");
        prompt.push_str("\n- Use memory_update when the user says \"remember this\", \"save this\", or asks you to store any information — this is the user's personal memory and they decide what goes in it");
        prompt.push_str("\n- CRITICAL: Before EVERY memory_update, you MUST call memory_get first. The memory file may contain important content from other sessions. Read it, merge your changes into the existing content, then write the complete updated markdown. Never overwrite blindly.");
        prompt.push_str(
            "\n- Keep memory organized with ## headings and bullet points",
        );
        prompt.push_str(
            "\n- Do not call memory_get at the start of every conversation — only when context is needed",
        );
    }

    prompt.push_str("\n");
    prompt.push_str("\nWhen the user asks about their work data (messages, tickets, pages, records), ALWAYS use the relevant tool to look it up. Never guess or answer from memory — the tools have real-time access to live data. If the user asks about a system not listed here, let them know that connector is not configured.");

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_config() {
        let config = IbexConfig::default();
        let prompt = build_system_prompt(&config);
        assert!(prompt.contains("IBEX"));
        assert!(prompt.contains("ALWAYS use the relevant tool"));
        assert!(!prompt.contains("Slack"));
        assert!(!prompt.contains("Jira"));
    }

    #[test]
    fn test_slack_only() {
        let mut config = IbexConfig::default();
        config.slack_token = Some("xoxp-test".to_string());
        let prompt = build_system_prompt(&config);
        assert!(prompt.contains("Slack: search messages"));
        assert!(!prompt.contains("Jira"));
    }

    #[test]
    fn test_all_connectors() {
        let mut config = IbexConfig::default();
        config.slack_token = Some("xoxp-test".to_string());
        config.notion_token = Some("ntn_test".to_string());
        config.jira_domain = Some("test.atlassian.net".to_string());
        config.jira_email = Some("test@test.com".to_string());
        config.jira_api_token = Some("token".to_string());
        config.github_token = Some("ghp_test".to_string());
        config.github_owner = Some("org".to_string());
        config.github_repo = Some("repo".to_string());
        config.servicenow_instance = Some("test.service-now.com".to_string());
        config.servicenow_username = Some("admin".to_string());
        config.servicenow_password = Some("pass".to_string());
        config.salesforce_instance_url = Some("https://test.my.salesforce.com".to_string());
        config.salesforce_access_token = Some("sf_token".to_string());

        let prompt = build_system_prompt(&config);
        assert!(prompt.contains("Slack"));
        assert!(prompt.contains("Notion"));
        assert!(prompt.contains("Jira"));
        assert!(prompt.contains("ServiceNow"));
        assert!(prompt.contains("Salesforce"));
        assert!(prompt.contains("Memory"));
        assert!(prompt.contains("memory_get"));
    }
}
