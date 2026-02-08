//! Shared email content templates
//!
//! Canonical content generators for invitation emails, used by both
//! production (SES) and mock email services.

/// Generate plain-text body for a team invitation email.
pub fn team_invitation_text(
    inviter_name: &str,
    team_name: &str,
    role: &str,
    invitation_url: &str,
) -> String {
    format!(
        "Hi there!\n\n\
        {} has invited you to join the team '{}' as a {}.\n\n\
        Click the link below to accept the invitation:\n\
        {}\n\n\
        This invitation will expire in 7 days.\n\n\
        If you don't have a Framecast account, you'll be prompted to create one.\n\n\
        Thanks,\n\
        The Framecast Team",
        inviter_name, team_name, role, invitation_url
    )
}

/// Generate styled HTML body for a team invitation email.
pub fn team_invitation_html(
    inviter_name: &str,
    team_name: &str,
    role: &str,
    invitation_url: &str,
) -> String {
    format!(
        r#"
            <html>
            <body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333;">
                <div style="max-width: 600px; margin: 0 auto; padding: 20px;">
                    <h2 style="color: #007cba;">You're invited to join {team_name}!</h2>

                    <p>Hi there!</p>

                    <p><strong>{inviter_name}</strong> has invited you to join the team '<strong>{team_name}</strong>' as a <strong>{role}</strong>.</p>

                    <div style="text-align: center; margin: 30px 0;">
                        <a href="{invitation_url}"
                           style="background-color: #007cba; color: white; padding: 12px 24px; text-decoration: none; border-radius: 4px; display: inline-block; font-weight: bold;">
                            Accept Invitation
                        </a>
                    </div>

                    <p>Or copy and paste this link in your browser:</p>
                    <p style="background-color: #f5f5f5; padding: 10px; border-radius: 4px; word-break: break-all;">
                        <a href="{invitation_url}">{invitation_url}</a>
                    </p>

                    <p style="color: #666; font-size: 14px;">
                        <em>This invitation will expire in 7 days.</em>
                    </p>

                    <hr style="border: none; border-top: 1px solid #eee; margin: 30px 0;">

                    <p style="color: #666; font-size: 12px;">
                        If you don't have a Framecast account, you'll be prompted to create one.<br>
                        Thanks, The Framecast Team
                    </p>
                </div>
            </body>
            </html>
            "#,
        team_name = team_name,
        inviter_name = inviter_name,
        role = role,
        invitation_url = invitation_url
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_team_invitation_text_contains_all_fields() {
        let text = team_invitation_text("Alice", "My Team", "admin", "https://example.com/accept");
        assert!(text.contains("Alice"));
        assert!(text.contains("My Team"));
        assert!(text.contains("admin"));
        assert!(text.contains("https://example.com/accept"));
        assert!(text.contains("7 days"));
    }

    #[test]
    fn test_team_invitation_html_contains_all_fields() {
        let html = team_invitation_html("Alice", "My Team", "admin", "https://example.com/accept");
        assert!(html.contains("Alice"));
        assert!(html.contains("My Team"));
        assert!(html.contains("admin"));
        assert!(html.contains("https://example.com/accept"));
        assert!(html.contains("7 days"));
    }
}
