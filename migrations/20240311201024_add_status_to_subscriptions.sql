-- Add migration script here
CREATE TYPE subscriptions_status AS ENUM ('pending_confirmation', 'confirmed');
ALTER TABLE subscriptions ADD COLUMN status subscriptions_status NULL;