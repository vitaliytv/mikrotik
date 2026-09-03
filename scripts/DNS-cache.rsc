# Keep RouterOS as the LAN caching resolver with provider DNS redundancy.
/ip dns set allow-remote-requests=yes servers=192.168.8.1,192.168.0.1 cache-size=4096KiB query-server-timeout=2s query-total-timeout=4s
