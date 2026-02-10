-- Add 'attempting' to webhook_delivery_status enum
-- to align with state machine
ALTER TYPE webhook_delivery_status
ADD VALUE IF NOT EXISTS 'attempting'
AFTER 'pending';
