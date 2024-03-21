#!/usr/bin/env bash

# source this scrip by calling it ./scripts/init_secrets.sh
export APP_EMAILCLIENT__TOKEN="$(cat ./secrets/postmark_token)"
export APP_EMAILCLIENT__BASE_URL="https://api.postmarkapp.com"