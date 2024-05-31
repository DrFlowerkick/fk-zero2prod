-- migrations/20240531191320_add_delivery_columns_to_newsletter_issues.sql
ALTER TABLE newsletter_issues ADD COLUMN num_current_subscribers INT;
ALTER TABLE newsletter_issues ADD COLUMN num_delivered_newsletters INT;
ALTER TABLE newsletter_issues ADD COLUMN num_failed_deliveries INT;