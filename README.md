# Covert Connect

> [!WARNING]
> Protocol is not finalized yet. Client and server must have the same version.

For now is just a pet project...

## Roadmap

- Add support for mobile devices
- Smarter load balancing: stick with the selected server if its ping is significantly better. Optionally, also monitor throughput—if it becomes too high, allow switching to other servers.
- Manage users (separate keys, restrict throughput, block) with web interface (docker image).

## Known issues

- SSH connections may disconnect after 1 minute. Because the project performs OSI Layer 3 to Layer 4 translation, it is difficult to detect and apply the correct TCP keepalive settings used by each system. As a workaround, configure SSH keepalive in your .ssh/config:
  
    ```sshconfig
    Host *
       ServerAliveInterval 30
       ServerAliveCountMax 3
    ```
