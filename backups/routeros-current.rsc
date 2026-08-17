# 2026-07-17 13:45:53 by RouterOS 7.23.1
# software id = 9DWX-CMV1
#
# model = C52iG-5HaxD2HaxD
# serial number = HJW0AKQSZP1
/interface bridge
add admin-mac=04:F4:1C:4F:F6:FB auto-mac=no comment=defconf name=bridge
/interface wifi
set [ find default-name=wifi1 ] channel.band=5ghz-ax .frequency=5785 \
    .skip-dfs-channels=10min-cac .width=20/40/80mhz configuration.mode=ap \
    .ssid=UneasyM disabled=no security.authentication-types=wpa2-psk,wpa3-psk \
    .ft=yes .ft-over-ds=yes .passphrase=<redacted>
set [ find default-name=wifi2 ] channel.band=2ghz-ax .skip-dfs-channels=\
    10min-cac .width=20/40mhz configuration.mode=ap .ssid=UneasyM disabled=no \
    security.authentication-types=wpa2-psk,wpa3-psk .ft=yes .ft-over-ds=yes \
    .passphrase=<redacted>
/interface ethernet switch
set switch1 cpu-flow-control=yes
/interface list
add comment=defconf name=WAN
add comment=defconf name=LAN
/ip pool
add name=default-dhcp ranges=192.168.88.10-192.168.88.254
/ip dhcp-server
add address-pool=default-dhcp interface=bridge name=defconf
/queue type
add kind=cake name=cake-qos
/queue simple
add comment="QoS upload AQM" disabled=yes max-limit=15000000/0 name=QoS-up \
    queue=cake-qos/default target=192.168.88.0/24
/routing table
add fib name=to_WAN1
add fib name=to_WAN2
/system logging action
set 0 memory-lines=5000
/system script
add dont-require-permissions=no name=setGlobals owner=admin policy=\
    ftp,reboot,read,write,policy,test,password,sniff,sensitive,romon source=":\
    global gwWAN1\
    \n:global gwWAN2\
    \n:set gwWAN1 192.168.0.1\
    \n:set gwWAN2 192.168.8.1\
    \n"
add dont-require-permissions=no name=finalize owner=admin policy=\
    ftp,reboot,read,write,policy,test,password,sniff,sensitive,romon source="/\
    interface/wifi/set [find name=wifi1] configuration.ssid=UneasyM security.p\
    assphrase=<redacted>
    \n:do { /interface/wifi/set [find name=wifi2] configuration.ssid=UneasyM s\
    ecurity.passphrase=<redacted> } on-error={}\
    \n:delay 2s\
    \n/system/reboot"
add comment="Symmetric dual-WAN health controller" \
    dont-require-permissions=no name=DUALWAN-health owner=admin policy=\
    read,write,policy,test source="# Exactly one RouterOS failover controller. Both WANs use the same health rule.\
    \n:if ([/system script job print count-only as-value where script=[:jobname]] > 1) do={ :error \"DUALWAN-health is already running\" }\
    \n\
    \n:global dwActiveBad\
    \n:global dwActiveState\
    \n:if ([:typeof \$dwActiveBad] = \"nothing\") do={ :set dwActiveBad 0 }\
    \n\
    \n# Initialize once after reboot from the installed main-route priority.\
    \n:if ([:typeof \$dwActiveState] = \"nothing\") do={\
    \n  :set dwActiveState \"bite\"\
    \n  :local lmtTables [/ip dhcp-client get [find where name=\"client2\"] default-route-tables]\
    \n  :if (([:len \$lmtTables] >= 6) && ([:pick \$lmtTables 0 6] = \"main:1\")) do={ :set dwActiveState \"lmt\" }\
    \n}\
    \n\
    \n:local current \$dwActiveState\
    \n:local activeInterface \"ether1\"\
    \n:if (\$current = \"lmt\") do={ :set activeInterface \"ether3\" }\
    \n:local edgeReceived [/ping address=212.93.105.242 interface=\$activeInterface count=3 interval=200ms]\
    \n:local publicReceived [/ping address=1.1.1.1 interface=\$activeInterface count=3 interval=200ms]\
    \n:local activeGood ((\$edgeReceived >= 2) || (\$publicReceived >= 2))\
    \n:local next \$current\
    \n:local reason \"active-healthy\"\
    \n\
    \n:if (\$activeGood) do={\
    \n  :set dwActiveBad 0\
    \n} else={\
    \n  :set dwActiveBad (\$dwActiveBad + 1)\
    \n  :set reason \"active-probes-degraded-keep-current\"\
    \n  :if (\$dwActiveBad >= 3) do={\
    \n    :if (\$current = \"lmt\") do={ :set next \"bite\" } else={ :set next \"lmt\" }\
    \n    :set dwActiveBad 0\
    \n    :set reason \"active-probes-degraded-3x-switch-next\"\
    \n  }\
    \n}\
    \n\
    \n:if (\$next != \$current) do={\
    \n  :if (\$next = \"bite\") do={\
    \n    /ip dhcp-client set [find where name=\"client1\"] default-route-tables=\"main:1,to_WAN1:1,to_WAN2:1\"\
    \n    :delay 1s\
    \n    /ip dhcp-client set [find where name=\"client2\"] default-route-tables=\"main:2,to_WAN1:2,to_WAN2:2\"\
    \n  } else={\
    \n    /ip dhcp-client set [find where name=\"client2\"] default-route-tables=\"main:1,to_WAN1:1,to_WAN2:2\"\
    \n    :delay 1s\
    \n    /ip dhcp-client set [find where name=\"client1\"] default-route-tables=\"main:2,to_WAN1:2,to_WAN2:1\"\
    \n  }\
    \n  :set dwActiveState \$next\
    \n  :log warning (\"DUALWAN state=\" . \$next . \" from=\" . \$current . \" reason=\" . \$reason . \" interface=\" . \$activeInterface . \" edge-received=\" . \$edgeReceived . \"/3 public-received=\" . \$publicReceived . \"/3\")\
    \n}"
/disk settings
set auto-media-interface=bridge auto-media-sharing=yes auto-smb-sharing=yes
/interface bridge port
add bridge=bridge comment=defconf interface=ether2
add bridge=bridge comment=defconf interface=ether4
add bridge=bridge comment=defconf interface=ether5
add bridge=bridge comment=defconf interface=wifi1
add bridge=bridge comment=defconf interface=wifi2
/ip neighbor discovery-settings
set discover-interface-list=LAN
/interface list member
add comment=defconf interface=bridge list=LAN
add comment=defconf interface=ether1 list=WAN
add comment=LB interface=ether3 list=WAN
/ip address
add address=192.168.88.1/24 comment=defconf interface=bridge network=\
    192.168.88.0
/ip dhcp-client
add comment=WAN2-Soyea default-route-tables=main:2,to_WAN1:2,to_WAN2:1 \
    interface=ether1 name=client1 use-peer-dns=no use-peer-ntp=no
add comment=WAN1-ZTE default-route-tables=main:1,to_WAN1:1,to_WAN2:2 \
    interface=ether3 name=client2 use-peer-dns=no use-peer-ntp=no
/ip dhcp-server network
add address=192.168.88.0/24 comment=defconf dns-server=192.168.88.1 gateway=\
    192.168.88.1
/ip dns
set allow-remote-requests=yes servers=1.0.0.1,9.9.9.9,8.8.4.4
/ip dns static
add address=192.168.88.1 comment=defconf name=router.lan type=A
/ip firewall filter
add action=accept chain=input comment=\
    "defconf: accept established,related,untracked" connection-state=\
    established,related,untracked
add action=drop chain=input comment="defconf: drop invalid" connection-state=\
    invalid
add action=accept chain=input comment="defconf: accept ICMP" protocol=icmp
add action=accept chain=input comment=\
    "defconf: accept to local loopback (for CAPsMAN)" dst-address=127.0.0.1
add action=drop chain=input comment="defconf: drop all not coming from LAN" \
    in-interface-list=!LAN
add action=accept chain=forward comment="defconf: accept in ipsec policy" \
    ipsec-policy=in,ipsec
add action=accept chain=forward comment="defconf: accept out ipsec policy" \
    ipsec-policy=out,ipsec
add action=fasttrack-connection chain=forward comment="defconf: fasttrack" \
    connection-state=established,related disabled=yes
add action=accept chain=forward comment=\
    "defconf: accept established,related, untracked" connection-state=\
    established,related,untracked
add action=drop chain=forward comment="defconf: drop invalid" \
    connection-state=invalid
add action=drop chain=forward comment=\
    "defconf: drop all from WAN not DSTNATed" connection-nat-state=!dstnat \
    connection-state=new in-interface-list=WAN
/ip firewall mangle
add action=mark-connection chain=input comment=LB:in1 in-interface=ether3 \
    new-connection-mark=WAN1_conn
add action=mark-connection chain=input comment=LB:in2 in-interface=ether1 \
    new-connection-mark=WAN2_conn
add action=mark-routing chain=output comment=LB:out1 connection-mark=\
    WAN1_conn new-routing-mark=*402
add action=mark-routing chain=output comment=LB:out2 connection-mark=\
    WAN2_conn new-routing-mark=*403
add action=mark-connection chain=prerouting comment=VOIP:dscp-ef \
    connection-state=new dscp=46 new-connection-mark=voip-conn src-address=\
    192.168.88.0/24
add action=mark-connection chain=prerouting comment=VOIP:dscp-cs4 \
    connection-state=new dscp=32 new-connection-mark=voip-conn src-address=\
    192.168.88.0/24
add action=mark-connection chain=prerouting comment=VOIP:dscp-af41 \
    connection-state=new dscp=34 new-connection-mark=voip-conn src-address=\
    192.168.88.0/24
add action=mark-connection chain=prerouting comment=VOIP:stun \
    connection-state=new dst-port=3478-3479 new-connection-mark=voip-conn \
    protocol=udp src-address=192.168.88.0/24
add action=mark-connection chain=prerouting comment=VOIP:zoom \
    connection-state=new dst-port=8801-8802 new-connection-mark=voip-conn \
    protocol=udp src-address=192.168.88.0/24
add action=mark-routing chain=prerouting comment=VOIP:route connection-mark=\
    voip-conn connection-state=new new-routing-mark=to_WAN1 passthrough=no
/ip firewall nat
add action=masquerade chain=srcnat comment="defconf: masquerade" \
    ipsec-policy=out,none out-interface-list=WAN
/ip route
add comment=DUALWAN-probe-lmt dst-address=212.93.105.242/32 gateway=\
    192.168.0.1 scope=10
add comment=DUALWAN-probe-lmt-public dst-address=1.1.1.1/32 gateway=\
    192.168.0.1 scope=10
add blackhole comment=DUALWAN-probe-lmt-blackhole distance=2 dst-address=\
    212.93.105.242/32
add blackhole comment=DUALWAN-probe-lmt-public-blackhole distance=2 \
    dst-address=1.1.1.1/32
/ipv6 firewall address-list
add address=::/128 comment="defconf: unspecified address" list=bad_ipv6
add address=::1/128 comment="defconf: lo" list=bad_ipv6
add address=fec0::/10 comment="defconf: site-local" list=bad_ipv6
add address=::ffff:0.0.0.0/96 comment="defconf: ipv4-mapped" list=bad_ipv6
add address=::/96 comment="defconf: ipv4 compat" list=bad_ipv6
add address=100::/64 comment="defconf: discard only " list=bad_ipv6
add address=2001:db8::/32 comment="defconf: documentation" list=bad_ipv6
add address=2001:10::/28 comment="defconf: ORCHID" list=bad_ipv6
add address=3ffe::/16 comment="defconf: 6bone" list=bad_ipv6
/ipv6 firewall filter
add action=accept chain=input comment=\
    "defconf: accept established,related,untracked" connection-state=\
    established,related,untracked
add action=drop chain=input comment="defconf: drop invalid" connection-state=\
    invalid
add action=accept chain=input comment="defconf: accept ICMPv6" protocol=\
    icmpv6
add action=accept chain=input comment="defconf: accept UDP traceroute" \
    dst-port=33434-33534 protocol=udp
add action=accept chain=input comment=\
    "defconf: accept DHCPv6-Client prefix delegation." dst-port=546 protocol=\
    udp src-address=fe80::/10
add action=accept chain=input comment="defconf: accept IKE" dst-port=500,4500 \
    protocol=udp
add action=accept chain=input comment="defconf: accept ipsec AH" protocol=\
    ipsec-ah
add action=accept chain=input comment="defconf: accept ipsec ESP" protocol=\
    ipsec-esp
add action=accept chain=input comment=\
    "defconf: accept all that matches ipsec policy" ipsec-policy=in,ipsec
add action=drop chain=input comment=\
    "defconf: drop everything else not coming from LAN" in-interface-list=\
    !LAN
add action=fasttrack-connection chain=forward comment="defconf: fasttrack6" \
    connection-state=established,related
add action=accept chain=forward comment=\
    "defconf: accept established,related,untracked" connection-state=\
    established,related,untracked
add action=drop chain=forward comment="defconf: drop invalid" \
    connection-state=invalid
add action=drop chain=forward comment=\
    "defconf: drop packets with bad src ipv6" src-address-list=bad_ipv6
add action=drop chain=forward comment=\
    "defconf: drop packets with bad dst ipv6" dst-address-list=bad_ipv6
add action=drop chain=forward comment="defconf: rfc4890 drop hop-limit=1" \
    hop-limit=equal:1 protocol=icmpv6
add action=accept chain=forward comment="defconf: accept ICMPv6" protocol=\
    icmpv6
add action=accept chain=forward comment="defconf: accept HIP" protocol=139
add action=accept chain=forward comment="defconf: accept IKE" dst-port=\
    500,4500 protocol=udp
add action=accept chain=forward comment="defconf: accept ipsec AH" protocol=\
    ipsec-ah
add action=accept chain=forward comment="defconf: accept ipsec ESP" protocol=\
    ipsec-esp
add action=accept chain=forward comment=\
    "defconf: accept all that matches ipsec policy" ipsec-policy=in,ipsec
add action=drop chain=forward comment=\
    "defconf: drop everything else not coming from LAN" in-interface-list=\
    !LAN
/system clock
set time-zone-name=Europe/Riga
/system scheduler
add comment="Symmetric dual-WAN scheduler" interval=5s name=\
    DUALWAN-health-every-5s on-event="/system script run DUALWAN-health" \
    policy=read,write,policy,test start-time=startup
/tool graphing interface
add allow-address=192.168.88.0/24
/tool graphing queue
add allow-address=192.168.88.0/24
/tool mac-server
set allowed-interface-list=LAN
/tool mac-server mac-winbox
set allowed-interface-list=LAN
