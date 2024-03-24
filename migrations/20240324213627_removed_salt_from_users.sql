-- migrations/20240324213627_removed_salt_from_users.sql
ALTER TABLE users DROP COLUMN salt;
