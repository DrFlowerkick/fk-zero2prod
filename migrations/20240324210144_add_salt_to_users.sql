-- migrations/20240324210144_add_salt_to_users.sql
ALTER TABLE users ADD COLUMN salt TEXT NOT NULL;
