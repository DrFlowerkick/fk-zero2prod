-- migrations/20240626200553_replace_email_with_user_id_in_issue_delivery_queue.sql
ALTER TABLE issue_delivery_queue DROP COLUMN subscriber_email;
ALTER TABLE issue_delivery_queue ADD COLUMN user_id uuid NOT NULL;