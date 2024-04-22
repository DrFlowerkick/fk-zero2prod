-- migrations/20240324202102_rename_password_column.sql
ALTER TABLE users RENAME password TO password_hash;
