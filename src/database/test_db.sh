#!/usr/bin/env bash

if ! which nc &>/dev/null; then
	echo "Netcat not installed"
	exit 1
fi

echo '{"command": "ENROLL", "checkpoint_id": 1, "worker_id": 1, "worker_name": "Bob", "worker_fingerprint": "this_is_a_hash", "location": "Halifax", "authorized_roles": "janitor", "role_id": 1}' | nc 127.0.0.1 3036
echo ""
echo '{"command": "AUTHENTICATE", "checkpoint_id": 1, "worker_id": 1, "worker_name": "Bob", "worker_fingerprint": "this_is_a_hash", "location": "Halifax", "authorized_roles": "janitor", "role_id": 1}' | nc 127.0.0.1 3036
echo ""
echo '{"command": "UPDATE", "checkpoint_id": 1, "worker_id": 1, "worker_name": "Bob", "worker_fingerprint": "this_is_a_hash", "location": "Halifax", "authorized_roles": "janitor", "role_id": 2}' | nc 127.0.0.1 3036
echo ""
echo '{"command": "AUTHENTICATE", "checkpoint_id": 1, "worker_id": 1, "worker_name": "Bob", "worker_fingerprint": "this_is_a_hash", "location": "Halifax", "authorized_roles": "janitor", "role_id": 1}' | nc 127.0.0.1 3036
echo ""
echo '{"command": "DELETE", "checkpoint_id": 1, "worker_id": 1, "worker_name": "Bob", "worker_fingerprint": "this_is_a_hash", "location": "Halifax", "authorized_roles": "janitor", "role_id": 1}' | nc 127.0.0.1 3036
