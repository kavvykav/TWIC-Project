# TWIC Access Control Project

## Requirements Analysis

1. The system must allow a worker past if he/she passes both authentication checks.
2. The system must deny a worker past the checkpoint if he/she fails an authentication check.
3. The system must use two factor authentication.
4. The system must have a fingerprint scanner.
5. The system must use a card for user authentication.
6. A server is needed to host a secure database of workers and their roles in the system.
7. The role of each user should determine their access to certain resources.
8. The biometric data of each registered worker will be stored locally at the port.
9. Data should not be stored in plaintext.
10. The port will keep logs of entries at all checkpoints.
11. Each checkpoint should be configurable to allow access only to those who need it.
12. The system must default to restricting access should an authentication mechanism fail.
13. The server should be able to get entry logs and register new users to each port when needed.
14. The server should be protected by password authentication so only those authorized can use it.

## User Categories

- Server manager: Is responsible for registering new users to ports and can request log data from a port. Has access to the central server.
- Port manager: Is responsible for the logs of all checkpoints and registering new workers to the port. Has access to the port logs and port user data.
- Local worker: Is an employee at their respective port and spends all day working there. Has access to what is needed for their job.
- Ship/train worker: Enters and leaves ports to deliver goods. Has access to enter and leave the port.
- Technician: Maintains the infrastructure of the port. Has access to resources that the port uses to function.
- Janitor/maintenance: Is responsible for maintaining the port. Has access to any part of the port that does not contain sensitive data or risks safety.

## Restrictions

1. No cloud databases due to vulnerabilities associated with it.
2. The security of the system should not be based on the secrecy of its design.
