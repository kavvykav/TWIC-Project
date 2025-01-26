#!/usr/bin/env bash

# For this test, initialize a checkpoint that is in Halifax and allows role id 1

if ! which nc &>/dev/null; then
	echo "Netcat not installed"
	exit 1
fi

echo '{"command": "ENROLL", "checkpoint_id": 1, "worker_id": 1, "worker_name": "Bob", "worker_fingerprint": "this_is_a_hash", "location": "Halifax", "authorized_roles": "janitor", "role_id": 1}' | nc 127.0.0.1 3036
echo '{"command": "ENROLL", "checkpoint_id": 1, "worker_id": 1, "worker_name": "Alice", "worker_fingerprint": "a_different_hash", "location": "Halifax", "authorized_roles": "janitor", "role_id": 1}' | nc 127.0.0.1 3036
echo '{"command": "ENROLL", "checkpoint_id": 1, "worker_id": 1, "worker_name": "Jeff", "worker_fingerprint": "an_even_differenter_hash", "location": "Vancouver", "authorized_roles": "janitor", "role_id": 1}' | nc 127.0.0.1 3036
echo '{"command": "ENROLL", "checkpoint_id": 1, "worker_id": 1, "worker_name": "Steve", "worker_fingerprint": "idk_what_to_call_this_hash", "location": "Halifax", "authorized_roles": "janitor", "role_id": 2}' | nc 127.0.0.1 3036
echo ""
cargo test
