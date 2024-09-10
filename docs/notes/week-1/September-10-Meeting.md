## Meeting On September 10, 2024

- Discussed potential issues with the existing card system that TWIC transport workers used (i.e. lost card, out of date, getting data off the chip since all biometric information is stored there)
- Came up with a random PIN alternative, where after the user is authenticated via a fingerprint scanner, a randomly generated PIN is sent to their phone. The access control system will then prompt the user to enter the PIN.
- This design will use a client/server model, where a server stores all enrolled individuals in a database, along with their biometrics. The server is responsible for managing who has permission for each client, and users for permission at each checkpoint will have their information stored at each local checkpoint. The discs will be encryped of course. Having permitted users stored on the client side allows for better performance, as there will be no network traffic  to worry about. Both layers of authentication are done locally on the client side.
- At the end of the meeting, sequence diagrams were made for the following use cases:
    1) Authentication of a transport worker
    2) An admin changing worker permissions for a specific station
