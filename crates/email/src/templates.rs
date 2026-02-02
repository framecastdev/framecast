//! Email Templates
//!
//! Provides reusable email templates for different types of emails
//! including team invitations, password resets, etc.

use std::collections::HashMap;
use uuid::Uuid;

use crate::{EmailError, EmailMessage};

/// Email template builder for team invitations
pub struct TeamInvitationTemplate {
    team_name: String,
    team_id: Uuid,
    invitation_id: Uuid,
    inviter_name: String,
    role: String,
    custom_message: Option<String>,
    expires_in_days: u32,
}

impl TeamInvitationTemplate {
    /// Create a new team invitation template
    pub fn new(
        team_name: String,
        team_id: Uuid,
        invitation_id: Uuid,
        inviter_name: String,
        role: String,
    ) -> Self {
        Self {
            team_name,
            team_id,
            invitation_id,
            inviter_name,
            role,
            custom_message: None,
            expires_in_days: 7,
        }
    }

    /// Add a custom message from the inviter
    pub fn with_custom_message(mut self, message: String) -> Self {
        self.custom_message = Some(message);
        self
    }

    /// Set expiration days (default: 7)
    pub fn with_expiration_days(mut self, days: u32) -> Self {
        self.expires_in_days = days;
        self
    }

    /// Build the email message
    pub fn build(
        &self,
        recipient_email: String,
        from_email: String,
    ) -> Result<EmailMessage, EmailError> {
        let invitation_url = format!(
            "https://framecast.app/teams/{}/invitations/{}/accept",
            self.team_id, self.invitation_id
        );

        let subject = format!("Invitation to join team: {}", self.team_name);

        let mut body_text = format!(
            "Hi there!\n\n\
            {} has invited you to join the team '{}' as a {}.\n\n",
            self.inviter_name, self.team_name, self.role
        );

        // Add custom message if provided
        if let Some(ref custom_msg) = self.custom_message {
            body_text.push_str(&format!(
                "Message from {}:\n\"{}\"\n\n",
                self.inviter_name, custom_msg
            ));
        }

        body_text.push_str(&format!(
            "Click the link below to accept the invitation:\n\
            {}\n\n\
            This invitation will expire in {} days.\n\n\
            If you don't have a Framecast account, you'll be prompted to create one.\n\n\
            Thanks,\n\
            The Framecast Team",
            invitation_url, self.expires_in_days
        ));

        let mut body_html = format!(
            r#"
            <html>
            <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>Team Invitation - {team_name}</title>
            </head>
            <body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333; margin: 0; padding: 0; background-color: #f4f4f4;">
                <div style="max-width: 600px; margin: 0 auto; background-color: #ffffff; padding: 0;">
                    <!-- Header -->
                    <div style="background-color: #007cba; padding: 20px; text-align: center;">
                        <h1 style="color: #ffffff; margin: 0; font-size: 24px;">Framecast</h1>
                    </div>

                    <!-- Content -->
                    <div style="padding: 30px;">
                        <h2 style="color: #007cba; margin-top: 0;">You're invited to join {team_name}!</h2>

                        <p style="margin-bottom: 20px;">Hi there!</p>

                        <p style="margin-bottom: 20px;">
                            <strong>{inviter_name}</strong> has invited you to join the team
                            '<strong>{team_name}</strong>' as a <strong>{role}</strong>.
                        </p>
            "#,
            team_name = self.team_name,
            inviter_name = self.inviter_name,
            role = self.role
        );

        // Add custom message section if provided
        if let Some(ref custom_msg) = self.custom_message {
            body_html.push_str(&format!(
                r#"
                        <div style="background-color: #f8f9fa; border-left: 4px solid #007cba; padding: 15px; margin: 20px 0;">
                            <p style="margin: 0; font-style: italic; color: #666;">
                                <strong>Message from {inviter_name}:</strong><br>
                                "{custom_msg}"
                            </p>
                        </div>
                "#,
                inviter_name = self.inviter_name,
                custom_msg = custom_msg
            ));
        }

        body_html.push_str(&format!(
            r#"
                        <div style="text-align: center; margin: 30px 0;">
                            <a href="{invitation_url}"
                               style="background-color: #007cba; color: white; padding: 15px 30px; text-decoration: none; border-radius: 5px; display: inline-block; font-weight: bold; font-size: 16px;">
                                Accept Invitation
                            </a>
                        </div>

                        <p style="margin-bottom: 10px;">Or copy and paste this link in your browser:</p>
                        <p style="background-color: #f5f5f5; padding: 12px; border-radius: 4px; word-break: break-all; font-family: monospace; font-size: 14px;">
                            <a href="{invitation_url}" style="color: #007cba; text-decoration: none;">{invitation_url}</a>
                        </p>

                        <div style="background-color: #fff3cd; border: 1px solid #ffeaa7; border-radius: 4px; padding: 15px; margin: 20px 0;">
                            <p style="margin: 0; color: #856404;">
                                ⏰ <strong>This invitation will expire in {expires_in_days} days.</strong>
                            </p>
                        </div>

                        <p style="color: #666; font-size: 14px; margin-bottom: 0;">
                            If you don't have a Framecast account, you'll be prompted to create one when you accept the invitation.
                        </p>
                    </div>

                    <!-- Footer -->
                    <div style="background-color: #f8f9fa; padding: 20px; text-align: center; border-top: 1px solid #dee2e6;">
                        <p style="margin: 0; color: #666; font-size: 12px;">
                            Thanks,<br>
                            The Framecast Team
                        </p>
                        <p style="margin: 10px 0 0 0; color: #999; font-size: 11px;">
                            This invitation was sent to {recipient_email}
                        </p>
                    </div>
                </div>
            </body>
            </html>
            "#,
            invitation_url = invitation_url,
            expires_in_days = self.expires_in_days,
            recipient_email = recipient_email
        ));

        let mut metadata = HashMap::new();
        metadata.insert("email_type".to_string(), "team_invitation".to_string());
        metadata.insert("team_id".to_string(), self.team_id.to_string());
        metadata.insert("invitation_id".to_string(), self.invitation_id.to_string());
        metadata.insert("role".to_string(), self.role.clone());
        metadata.insert("template_version".to_string(), "1.0".to_string());

        Ok(
            EmailMessage::new(recipient_email, from_email, subject, body_text)
                .with_html(body_html)
                .with_metadata("email_type".to_string(), "team_invitation".to_string())
                .with_metadata("team_id".to_string(), self.team_id.to_string())
                .with_metadata("invitation_id".to_string(), self.invitation_id.to_string())
                .with_metadata("role".to_string(), self.role.clone())
                .with_metadata("template_version".to_string(), "1.0".to_string()),
        )
    }
}

/// Email template for password reset
pub struct PasswordResetTemplate {
    user_name: Option<String>,
    reset_token: String,
    expires_in_hours: u32,
}

impl PasswordResetTemplate {
    pub fn new(reset_token: String) -> Self {
        Self {
            user_name: None,
            reset_token,
            expires_in_hours: 24,
        }
    }

    pub fn with_user_name(mut self, name: String) -> Self {
        self.user_name = Some(name);
        self
    }

    pub fn with_expiration_hours(mut self, hours: u32) -> Self {
        self.expires_in_hours = hours;
        self
    }

    pub fn build(
        &self,
        recipient_email: String,
        from_email: String,
    ) -> Result<EmailMessage, EmailError> {
        let reset_url = format!(
            "https://framecast.app/auth/reset-password?token={}", // pragma: allowlist secret
            self.reset_token                                      // pragma: allowlist secret
        );

        let greeting = match &self.user_name {
            Some(name) => format!("Hi {},", name),
            None => "Hi there,".to_string(),
        };

        let subject = "Reset your Framecast password".to_string();

        let body_text = format!(
            "{}\n\n\
            We received a request to reset your Framecast password.\n\n\
            Click the link below to reset your password:\n\
            {}\n\n\
            This link will expire in {} hours.\n\n\
            If you didn't request a password reset, you can safely ignore this email.\n\n\
            Thanks,\n\
            The Framecast Team",
            greeting, reset_url, self.expires_in_hours
        );

        let body_html = format!(
            r#"
            <html>
            <body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333;">
                <div style="max-width: 600px; margin: 0 auto; padding: 20px;">
                    <h2 style="color: #007cba;">Reset your Framecast password</h2>

                    <p>{greeting}</p>

                    <p>We received a request to reset your Framecast password.</p>

                    <div style="text-align: center; margin: 30px 0;">
                        <a href="{reset_url}"
                           style="background-color: #007cba; color: white; padding: 12px 24px; text-decoration: none; border-radius: 4px; display: inline-block; font-weight: bold;">
                            Reset Password
                        </a>
                    </div>

                    <p>Or copy and paste this link in your browser:</p>
                    <p style="background-color: #f5f5f5; padding: 10px; border-radius: 4px; word-break: break-all;">
                        <a href="{reset_url}">{reset_url}</a>
                    </p>

                    <p style="color: #666; font-size: 14px;">
                        <em>This link will expire in {expires_in_hours} hours.</em>
                    </p>

                    <hr style="border: none; border-top: 1px solid #eee; margin: 30px 0;">

                    <p style="color: #666; font-size: 12px;">
                        If you didn't request a password reset, you can safely ignore this email.<br>
                        Thanks, The Framecast Team
                    </p>
                </div>
            </body>
            </html>
            "#,
            greeting = greeting,
            reset_url = reset_url,
            expires_in_hours = self.expires_in_hours
        );

        let mut metadata = HashMap::new();
        metadata.insert("email_type".to_string(), "password_reset".to_string());
        metadata.insert("template_version".to_string(), "1.0".to_string());

        Ok(
            EmailMessage::new(recipient_email, from_email, subject, body_text)
                .with_html(body_html)
                .with_metadata("email_type".to_string(), "password_reset".to_string())
                .with_metadata("template_version".to_string(), "1.0".to_string()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_team_invitation_template() {
        let team_id = Uuid::new_v4();
        let invitation_id = Uuid::new_v4();

        let template = TeamInvitationTemplate::new(
            "Test Team".to_string(),
            team_id,
            invitation_id,
            "John Doe".to_string(),
            "member".to_string(),
        )
        .with_custom_message("Welcome to our awesome team!".to_string())
        .with_expiration_days(14);

        let message = template
            .build(
                "invitee@example.com".to_string(),
                "invitations@framecast.app".to_string(),
            )
            .unwrap();

        assert_eq!(message.to, "invitee@example.com");
        assert_eq!(message.subject, "Invitation to join team: Test Team");
        assert!(message.body_text.contains("John Doe"));
        assert!(message.body_text.contains("Test Team"));
        assert!(message.body_text.contains("member"));
        assert!(message.body_text.contains("Welcome to our awesome team!"));
        assert!(message.body_text.contains("14 days"));

        assert!(message.body_html.is_some());
        let html = message.body_html.unwrap();
        assert!(html.contains("Test Team"));
        assert!(html.contains("John Doe"));
        assert!(html.contains("Welcome to our awesome team!"));

        assert_eq!(
            message.metadata.get("email_type"),
            Some(&"team_invitation".to_string())
        );
        assert_eq!(message.metadata.get("team_id"), Some(&team_id.to_string()));
        assert_eq!(
            message.metadata.get("invitation_id"),
            Some(&invitation_id.to_string())
        );
    }

    #[test]
    fn test_password_reset_template() {
        let template = PasswordResetTemplate::new("reset_token_123".to_string())
            .with_user_name("Jane Smith".to_string())
            .with_expiration_hours(12);

        let message = template
            .build(
                "user@example.com".to_string(),
                "noreply@framecast.app".to_string(),
            )
            .unwrap();

        assert_eq!(message.to, "user@example.com");
        assert_eq!(message.subject, "Reset your Framecast password");
        assert!(message.body_text.contains("Hi Jane Smith,"));
        assert!(message.body_text.contains("reset_token_123"));
        assert!(message.body_text.contains("12 hours"));

        assert!(message.body_html.is_some());
        let html = message.body_html.unwrap();
        assert!(html.contains("Jane Smith"));
        assert!(html.contains("reset_token_123"));

        assert_eq!(
            message.metadata.get("email_type"),
            Some(&"password_reset".to_string())
        );
    }
}
