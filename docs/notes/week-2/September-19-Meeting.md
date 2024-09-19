**Discussion on Authentication**

While discussion remains ongoing on the authentication required for the system the team has identified three components to be used a Personal PIN, Card, Fingerprint.
Discussion then remains on where the deployment of these components should occur namely should a PIN and Card or a Fingerprint and Card be used for access to the
port. Then within the port a card will be used to access low or medium secuirty areas (or nothing at all for low secuirty) and for high secuirty a 
Fingerprint and Card (in the case of PIN and card to enter port) or a PIN and Card (in the case of Fingerprint and Card to enter port) will be used to access.

**Argument for PIN and Card Entry**



**Argument for Fingerprint and Card Entry**






**RUST** - Rust is currently seen as a strong choice for the programming. It handles synchronization well while providing memory security which is greatly needed
in a highly security based system. It is nearly impossible to accidentally create vulnerabilities that would allow bad actors the necessary access to manipulate 
or extract data. Wrappers already exist to facilitate communication between MySQL and Rust (MySQL is currently planned as the database manager). It also has 
the ability to communicate well with C programms which is something the team is already sharp in. 
