# September 25 Meeting

- Debate on which methods of authentication are needed for which security levels
- Different user classes: each user needs the minimum permissions to do their job
- Also need to consider the burden on users
- Try to follow fundamental design principles (refer to SYSC4810 Module 1.2) for ideas
- Need to check user permissions at every checkpoint (least privilige), need multifactor authentication
- If access control fails (i.e. door is propped open), raise an alarm
- PIN isn't necessarily more or less secure than a biometric, there are pros and cons to both
- More or Less secure buildings - what does that mean? 
- Should limit to 2 layers to avoid burdening the user
- May not follow design principles to a tee but as engineers we need to make decisions and be able to justify them
- 6 or 8 digit PIN instead of multiple PINs seems to be more favourable
- Write some more clear requirements, meet this week to discuss this
- Look up access control types

## Access Control Crash Course Notes

- Verify identity via passwords, tokens, biometric (consider different types)
- Fingerprint gives a good balance between accuracy and cost, middle ground for both criteria
- Enrollment, verification, identification for biometric sensors
- Local vs Remote User authentication
    1) Challenge Response Protocol for Remote authentication: protects the password from being sent in plaintext
    2) Can work for tokens as well
    3) For biometric sensors, the device is also authenticated to make sure there isn't a recording or something
    4) Will need to encrpyt or has passwords when sending them over the network
- Access control- Authentication function and access control function
- Discretionary (identity based on the requestor), role based, attribute based (attributes of the user, the resource, and current environmental conditions)
- Mandatory access control -> based on security clearance
- Find Stallings Security textbook for any additional concepts

