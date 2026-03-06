//! System prompt generation — Rust port of scripts/build-prompt.sh.
//!
//! Builds the system prompt dynamically based on which connectors
//! are configured in IbexConfig.

use crate::config::IbexConfig;

/// Build the system prompt based on configured connectors.
/// Tool names here match what Open WebUI exposes to the LLM:
/// the server ID (e.g. "slack") is prepended to each tool name
/// (e.g. "search_messages") → "slack_search_messages".
pub fn build_system_prompt(config: &IbexConfig) -> String {
    let mut prompt = String::from(
        "You are IBEX, a workplace AI assistant with access to real-time company tools.\n\
         \n\
         IMPORTANT RULES:\n\
         - When the user asks about work data (messages, tickets, pages, records), ALWAYS use the relevant tool. Never guess.\n\
         - You may need to chain multiple tool calls. For example: search first, then get details.\n\
         - If a tool returns an error, tell the user what went wrong.\n\
         - If the user asks about a system not listed below, tell them that connector is not configured.\n",
    );

    if config.is_slack_configured() {
        prompt.push_str(
            "\n## Slack Tools\n\
             Use these to find and read Slack messages:\n\
             - **slack_search_messages**(query, count?) — Search messages across all channels. Use this FIRST for any Slack question.\n\
             - **slack_get_channel_history**(channel_id, limit?) — Get recent messages from a specific channel. Requires a channel_id.\n\
             - **slack_list_channels**(types?) — List channels with their IDs. Use when you need to find a channel_id.\n\
             - **slack_get_thread**(channel_id, thread_ts) — Get all replies in a thread.\n\
             \n\
             Slack workflow: To find what someone said → use search_messages. To read a channel → list_channels to get the ID, then get_channel_history.\n",
        );
    }

    if config.is_notion_configured() {
        prompt.push_str(
            "\n## Notion Tools\n\
             Use these to find and read Notion pages:\n\
             - **notion_search**(query, filter?) — Search pages and databases by keyword.\n\
             - **notion_get_page**(page_id) — Get a full Notion page with all content.\n\
             - **notion_get_block_children**(block_id, recursive?) — Get child blocks of a page or block.\n\
             - **notion_query_database**(database_id, filter?, sorts?) — Query a Notion database with filters.\n",
        );
    }

    if config.is_jira_configured() {
        prompt.push_str(
            "\n## Jira Tools\n\
             Use these to find and read Jira issues:\n\
             - **jira_search_issues**(jql, max_results?) — Search issues using JQL. Examples: 'assignee = currentUser()', 'project = PROJ AND status = Open'.\n\
             - **jira_get_issue**(issue_key) — Get full details of an issue (e.g. 'PROJ-1234').\n\
             - **jira_get_projects**() — List all accessible Jira projects.\n",
        );
    }

    if config.is_servicenow_configured() {
        prompt.push_str(
            "\n## ServiceNow Tools\n\
             - **servicenow_query_table**(table, query?, fields?, limit?) — Query a table (e.g. incident, sys_user).\n\
             - **servicenow_get_record**(table, sys_id) — Get a specific record.\n\
             - **servicenow_list_tables**() — List common tables.\n",
        );
    }

    if config.is_salesforce_configured() {
        prompt.push_str(
            "\n## Salesforce Tools\n\
             - **salesforce_soql_query**(query, limit?) — Run a SOQL query.\n\
             - **salesforce_get_record**(object_type, record_id, fields?) — Get a record.\n\
             - **salesforce_search**(query, limit?) — Global search across objects.\n\
             - **salesforce_describe_object**(object_type) — Get schema/fields of an object.\n\
             - **salesforce_list_objects**() — List available objects.\n",
        );
    }

    if config.is_memory_configured() {
        prompt.push_str(
            "\n## Memory Tools\n\
             Persistent memory stored in GitHub:\n\
             - **memory_get**() — Read current memory contents.\n\
             - **memory_update**(content, message?) — Replace memory with new markdown.\n\
             \n\
             Memory rules:\n\
             - Use memory_get when the user references previous context or asks \"what do you know\".\n\
             - Use memory_update when the user says \"remember this\" or \"save this\".\n\
             - CRITICAL: Before EVERY memory_update, call memory_get first. Read existing content, merge changes, then write complete markdown.\n\
             - Keep memory organized with ## headings and bullet points.\n\
             - Do not call memory_get at the start of every conversation — only when context is needed.\n",
        );
    }

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
        assert!(prompt.contains("slack_search_messages"));
        assert!(prompt.contains("slack_get_channel_history"));
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
        config.salesforce_username = Some("admin@test.com".to_string());
        config.salesforce_password = Some("pass".to_string());

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
