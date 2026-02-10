-- Revert: Remove 'attempting' from webhook_delivery_status enum
-- NOTE: PostgreSQL does not support ALTER TYPE DROP VALUE.
-- Workaround: recreate the enum without 'attempting'.

-- Move any 'attempting' rows back to 'pending'
UPDATE webhook_deliveries SET status = 'pending'
WHERE status = 'attempting';

-- Rename old type, create new without 'attempting', migrate column, drop old
ALTER TYPE webhook_delivery_status RENAME TO webhook_delivery_status_old;
CREATE TYPE webhook_delivery_status AS ENUM (
    'pending', 'retrying', 'delivered', 'failed'
);
ALTER TABLE webhook_deliveries
ALTER COLUMN status TYPE webhook_delivery_status
USING status::text::webhook_delivery_status;
DROP TYPE webhook_delivery_status_old;
