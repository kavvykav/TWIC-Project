#!/usr/bin/env bash

if ! which nc &>/dev/null; then
	echo "Netcat not installed"
	exit 1
fi

# Should pass
echo '{"command": "ENROLL", "checkpoint_id": 1000, "worker_id": 1, "worker_name": "Bob", "worker_fingerprint": "this_is_a_hash", "location": "Halifax", "authorized_roles": "janitor", "role_id": 1}' | nc 127.0.0.1 3036
echo ""
# Should fail
echo '{"command": "ENROLL", "checkpoint_id": 1000, "worker_id": 1, "worker_name": "Bob", "worker_fingerprint": "this_is_a_hash", "location": "Halifax", "authorized_roles": "janitor", "role_id": 1}' | nc 127.0.0.1 3036
echo ""
# Should pass
echo '{"command": "AUTHENTICATE", "checkpoint_id": 1000, "worker_id": 1, "worker_name": "Bob", "worker_fingerprint": "this_is_a_hash", "location": "Halifax", "authorized_roles": "janitor", "role_id": 1}' | nc 127.0.0.1 3036
echo ""
# Should pass
echo '{"command": "UPDATE", "checkpoint_id": 1000, "worker_id": 1, "worker_name": "Bob", "worker_fingerprint": "this_is_a_hash", "location": "Halifax", "authorized_roles": "janitor", "role_id": 2}' | nc 127.0.0.1 3036
echo ""
# Should pass
echo '{"command": "AUTHENTICATE", "checkpoint_id": 1000, "worker_id": 1, "worker_name": "Bob", "worker_fingerprint": "this_is_a_hash", "location": "Halifax", "authorized_roles": "janitor", "role_id": 1}' | nc 127.0.0.1 3036
echo ""
# Should pass
echo '{"command": "DELETE", "checkpoint_id": 1000, "worker_id": 1, "worker_name": "Bob", "worker_fingerprint": "this_is_a_hash", "location": "Halifax", "authorized_roles": "janitor", "role_id": 1}' | nc 127.0.0.1 3036
echo ""
# Should fail
echo '{"command": "DELETE", "checkpoint_id": 1000, "worker_id": 1, "worker_name": "Bob", "worker_fingerprint": "this_is_a_hash", "location": "Halifax", "authorized_roles": "janitor", "role_id": 1}' | nc 127.0.0.1 3036
