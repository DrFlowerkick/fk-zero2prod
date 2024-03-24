-- migrations/20240311201024_add_status_to_subscriptions.sql
CREATE TYPE subscriptions_status AS ENUM ('pending_confirmation', 'confirmed');
ALTER TABLE subscriptions ADD COLUMN status subscriptions_status NULL;