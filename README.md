# TWIC Access Control Project

The TWIC access control system is a two-factor authentication system for Canadian
maritime ports, written in Rust. Below you can find instructions for installing
and using each component of the TWIC system.

## Getting Started

In depth documentation can be found in our [docs](./docs/README.md).

### Installation

Clone the repo to your machine.

```bash
git clone https://github.com/kavvykav/TWIC-Project.git
```

### Usage

#### Main Server

The code for the Main Server is located in src/database.

```bash
cd src/database
cargo run
```

#### Port Server

The code for the Port Server is located at src/port_server.

```bash
cd src/port_server
cargo run
```

#### Checkpoint

> [!IMPORTANT]
> The checkpoint code must be run on a Raspberry Pi connected to an RFID and
> fingerprint sensor as shown in the instructions.

The code for the Checkpoint is located at src/checkpoint. The checkpoint takes
command line arguments for the type of checkpoint it is, the roles it allows,
and its location. More in depth instructions can be found [here](./src/checkpoint/README.md).

```bash
cd src/checkpoint
cargo run [function] [location] [allowed_roles]
```
