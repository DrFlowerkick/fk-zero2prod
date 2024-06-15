-- migrations/20240609133113_add_trey_columns_to_issue_delivery_queue.sql
ALTER TABLE issue_delivery_queue ADD COLUMN n_retries SMALLINT NOT NULL;
ALTER TABLE issue_delivery_queue ADD COLUMN execute_after timestamptz NOT NULL;
