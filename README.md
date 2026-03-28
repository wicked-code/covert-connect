# Covert Connect

> [!WARNING]
> Protocol is not finalized yet. Client and server must have the same version.

For now is just a pet project...

## Roadmap

- Migrate to use OSI Layer 3 instead of a proxy
- Add support for mobile devices
- Smarter load balancing: stick with the selected server if its ping is significantly better. Optionally, also monitor throughput—if it becomes too high, allow switching to other servers.
- Manage users (seraprate keys, restrict throughtput, block) with web interface (docker image).
