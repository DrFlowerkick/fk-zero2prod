-- migrations/20240414165630_seed_user.sql
INSERT INTO users (user_id, username, password_hash)
VALUES (
    '018b7120-4509-4348-9850-a9cc62ba3ce2',
    'admin',
    '$argon2id$v=19$m=15000,t=2,p=1$5mCHVA68B8AaqP6rc/IGDA$3D4izGvFN/ZXBc8uAMqYFiE9TxcZ4AaSdoio4zXuKZQ'
);