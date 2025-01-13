#!/usr/bin/env bash

if ! which nc &>/dev/null; then
	echo "Netcat not installed"
	exit 1
fi

echo '{"command": "ENROLL", "data": "Alice Brown,Port F,Manager"}' | nc 127.0.0.1 3036
echo ""
echo '{"command": "AUTHENTICATE", "data": "1"}' | nc 127.0.0.1 3036
echo ""
echo '{"command": "UPDATE", "data": "1,Admin"}' | nc 127.0.0.1 3036
echo ""
echo '{"command": "AUTHENTICATE", "data": "1"}' | nc 127.0.0.1 3036
echo ""
echo '{"command": "DELETE", "data": "1"}' | nc 127.0.0.1 3036
