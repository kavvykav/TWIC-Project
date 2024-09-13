# TWIC Access Control Project

## Requirements Analysis
1) The system must allow a worker past if he/she passes both authentication checks.
2) The system must deny a worker past the checkpoint if he/she fails an authentication check.
3) The system must have a fingerprint scanner.
4) The system must use two factor authentication.
5) The system must use a randomly generated PIN sent to the worker's cell phone as a secondary form of authentication.
6) A server is needed to host a secure database of workers in the system.
7) Permissions of each worker should be managed by the server.
8) The system should store each permitted worker's data in their respective local location.
9) Encryption or hashing in the database is needed, so if a passive attack occurs, there isn't a catastrophic leak.
10) The system must log events locally. Each port will keep track of authentication attempts independently of each other.
11) In the event of one of the authentication layers failing, the system must remain closed.
12) The system must have a web application for administrators to change permissions for different ports as needed.
13) The web application must be password protected so only administrators can utilize it.

## Restrictions
1) No cloud databases due to vulnerabilities associated with it.
2) The security of the system should not be based on the secrecy of its design.
