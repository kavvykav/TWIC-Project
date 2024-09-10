# TWIC Access Control Project

## Requirements Analysis
1) A server is needed to host a secure database. We will need to have a master/slave configuration for the master to send biometric data to the slave.
2) Encryption or hashing in the database is needed, so if a passive attack occurs, there isn't a catastrophic leak.
3) Two factor authentication is needed. One factor will be a fingerprint sensor. The other will either could potentially be the system sending a randomly generated 6 digit PIN to the transportation worker after verifying the fingerprint. The system will send the PIN to the phone number associated with the fingerprint read if the fingerprint is stored on the database. This will also require a change in the enrollment process. This will eliminate the issues associated with the card system.
4) Needs Raspberry Pis with networking capibilities, running some Linux distro. The discs in each Raspberry Pi need to be encrypted.

## Restrictions
1) No cloud databases due to vulnerabilities associated with it.
2) The security of the system should not be based on the secrecy of its design.
