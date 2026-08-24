# 2026-08-21 15:00:14 by RouterOS 7.23.3
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
add disk-file-count=20 disk-file-name=dualwan-history name=dualwandisk \
    target=disk
add disk-file-count=20 disk-file-name=wan-quality name=wanqualitydisk target=\
    disk
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
    assphrase=<redacted>\
    \n:do { /interface/wifi/set [find name=wifi2] configuration.ssid=UneasyM s\
    ecurity.passphrase=<redacted> } on-error={}\
    \n:delay 2s\
    \n/system/reboot"
add comment="Symmetric active-WAN health controller (3x degraded cycles)" \
    dont-require-permissions=no name=DUALWAN-health owner=admin policy=\
    read,write,policy,test source="# Exactly one RouterOS failover controller.\
    \_Both WANs use the same health rule.\
    \n:if ([/system script job print count-only as-value where script=[:jobnam\
    e]] > 1) do={ :error \"DUALWAN-health is already running\" }\
    \n\
    \n:global dwActiveBad\
    \n:global dwActiveState\
    \n:if ([:typeof \$dwActiveBad] = \"nothing\") do={ :set dwActiveBad 0 }\
    \n\
    \n# Initialize once after reboot from the installed main-route priority.\
    \n:if ([:typeof \$dwActiveState] = \"nothing\") do={\
    \n  :set dwActiveState \"bite\"\
    \n  :local lmtTables [/ip dhcp-client get [find where name=\"client2\"] de\
    fault-route-tables]\
    \n  :if (([:len \$lmtTables] >= 6) && ([:pick \$lmtTables 0 6] = \"main:1\
    \")) do={ :set dwActiveState \"lmt\" }\
    \n}\
    \n\
    \n:local current \$dwActiveState\
    \n:local activeInterface \"ether1\"\
    \n:if (\$current = \"lmt\") do={ :set activeInterface \"ether3\" }\
    \n:local edgeReceived [/ping address=212.93.105.242 interface=\$activeInte\
    rface count=3 interval=200ms]\
    \n:local publicReceived [/ping address=1.1.1.1 interface=\$activeInterface\
    \_count=3 interval=200ms]\
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
    \n    :if (\$current = \"lmt\") do={ :set next \"bite\" } else={ :set next\
    \_\"lmt\" }\
    \n    :set dwActiveBad 0\
    \n    :set reason \"active-probes-degraded-3x-switch-next\"\
    \n  }\
    \n}\
    \n\
    \n:if (\$next != \$current) do={\
    \n  :if (\$next = \"bite\") do={\
    \n    /ip dhcp-client set [find where name=\"client1\"] default-route-tabl\
    es=\"main:1,to_WAN1:1,to_WAN2:1\"\
    \n    :delay 1s\
    \n    /ip dhcp-client set [find where name=\"client2\"] default-route-tabl\
    es=\"main:2,to_WAN1:2,to_WAN2:2\"\
    \n  } else={\
    \n    /ip dhcp-client set [find where name=\"client2\"] default-route-tabl\
    es=\"main:1,to_WAN1:1,to_WAN2:2\"\
    \n    :delay 1s\
    \n    /ip dhcp-client set [find where name=\"client1\"] default-route-tabl\
    es=\"main:2,to_WAN1:2,to_WAN2:1\"\
    \n  }\
    \n  :set dwActiveState \$next\
    \n  :log warning (\"DUALWAN state=\" . \$next . \" from=\" . \$current . \
    \" reason=\" . \$reason . \" interface=\" . \$activeInterface . \" edge-re\
    ceived=\" . \$edgeReceived . \"/3 public-received=\" . \$publicReceived . \
    \"/3\")\
    \n}\
    \n"
add dont-require-permissions=no name=WAN-quality-sync owner=admin policy=\
    read,write,policy,test source="# Keep monitor-only Netwatch probes pinned \
    to the current DHCP address of each WAN.\
    \n:if ([/system script job print count-only as-value where script=[:jobnam\
    e]] > 1) do={ :error \"WAN-quality-sync is already running\" }\
    \n\
    \n:local syncWan do={\
    \n  :local wan \$1\
    \n  :local clientName \$2\
    \n  :local tableName \$3\
    \n  :local clientId [/ip dhcp-client find where name=\$clientName]\
    \n  :if ([:len \$clientId] != 1) do={ :error (\"WANQUALITY sync missing DH\
    CP client=\" . \$clientName) }\
    \n  :local addressWithMask [/ip dhcp-client get \$clientId address]\
    \n  :local slash [:find \$addressWithMask \"/\"]\
    \n  :if ([:typeof \$slash] = \"nil\") do={ :error (\"WANQUALITY sync missi\
    ng address wan=\" . \$wan) }\
    \n  :local address [:pick \$addressWithMask 0 \$slash]\
    \n\
    \n  :local ruleComment (\"WANQUALITY route \" . \$wan)\
    \n  :local ruleIds [/routing rule find where comment=\$ruleComment]\
    \n  :if ([:len \$ruleIds] != 1) do={ :error (\"WANQUALITY sync expected on\
    e route rule wan=\" . \$wan) }\
    \n  /routing rule set \$ruleIds src-address=(\$address . \"/32\") action=l\
    ookup-only-in-table table=\$tableName\
    \n\
    \n  :local probeComment (\"WANQUALITY probe \" . \$wan)\
    \n  :local probeIds [/tool netwatch find where comment=\$probeComment]\
    \n  :if ([:len \$probeIds] != 5) do={ :error (\"WANQUALITY sync expected f\
    ive probes wan=\" . \$wan) }\
    \n  /tool netwatch set \$probeIds src-address=\$address\
    \n}\
    \n\
    \n\$syncWan \"lmt\" \"client2\" \"to_WAN1\"\
    \n\$syncWan \"bite\" \"client1\" \"to_WAN2\"\
    \n"
add comment="Passive WAN interface counters" dont-require-permissions=no \
    name=WAN-quality-interface owner=admin policy=read,test source="# Persist \
    passive physical-interface counters without generating network traffic.\
    \n:if ([/system script job print count-only as-value where script=[:jobnam\
    e]] > 1) do={ :error \"WAN-quality-interface is already running\" }\
    \n\
    \n:local logInterface do={\
    \n  :local wan \$1\
    \n  :local interfaceName \$2\
    \n  :local interfaceId [/interface find where name=\$interfaceName]\
    \n  :local ethernetId [/interface ethernet find where name=\$interfaceName\
    ]\
    \n  :if ([:len \$interfaceId] != 1) do={ :error (\"WANQUALITY interface mi\
    ssing interface=\" . \$interfaceName) }\
    \n  :if ([:len \$ethernetId] != 1) do={ :error (\"WANQUALITY interface mis\
    sing ethernet=\" . \$interfaceName) }\
    \n\
    \n  :local running [/interface get \$interfaceId running]\
    \n  :local queueDrops [/interface get \$interfaceId tx-queue-drop]\
    \n  :local linkDowns [/interface get \$interfaceId link-downs]\
    \n  :local fcsErrors [/interface ethernet get \$ethernetId rx-fcs-error]\
    \n  :local alignErrors [/interface ethernet get \$ethernetId rx-align-erro\
    r]\
    \n  :local collisions [/interface ethernet get \$ethernetId tx-collision]\
    \n  :log info (\"WANQUALITY type=interface wan=\" . \$wan . \" interface=\
    \" . \$interfaceName . \" running=\" . \$running . \" tx_queue_drop=\" . \
    \$queueDrops . \" link_downs=\" . \$linkDowns . \" rx_fcs_error=\" . \$fcs\
    Errors . \" rx_align_error=\" . \$alignErrors . \" tx_collision=\" . \$col\
    lisions)\
    \n}\
    \n\
    \n\$logInterface \"lmt\" \"ether3\"\
    \n\$logInterface \"bite\" \"ether1\"\
    \n"
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
set allow-remote-requests=yes servers=8.8.8.8,8.8.4.4
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
add comment=DUALWAN-probe-lmt disabled=no dst-address=212.93.105.242/32 \
    gateway=192.168.0.1 scope=10
add comment=DUALWAN-probe-lmt-public disabled=no dst-address=1.1.1.1/32 \
    gateway=192.168.0.1 scope=10
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
/routing rule
add action=lookup-only-in-table comment="WANQUALITY route lmt" src-address=\
    192.168.0.215/32 table=to_WAN1
add action=lookup-only-in-table comment="WANQUALITY route bite" src-address=\
    192.168.8.126/32 table=to_WAN2
/system clock
set time-zone-name=Europe/Riga
/system logging
add action=dualwandisk regex="^DUALWAN state=" topics=script,warning
add action=wanqualitydisk regex="^WANQUALITY " topics=script,info
/system scheduler
add comment="Symmetric dual-WAN health scheduler" interval=5s name=\
    DUALWAN-health-every-5s on-event="/system script run DUALWAN-health" \
    policy=read,write,policy,test start-time=startup
add comment="Refresh monitor-only WAN quality source addresses" interval=5m \
    name=WAN-quality-sync-every-5m on-event=\
    "/system script run WAN-quality-sync" policy=read,write,policy,test \
    start-time=startup
add comment="Passive WAN interface counters" interval=1h name=\
    WAN-quality-interface-every-1h on-event=\
    "/system script run WAN-quality-interface" policy=read,test start-time=\
    startup
/tool graphing interface
add allow-address=192.168.88.0/24
/tool graphing queue
add allow-address=192.168.88.0/24
/tool mac-server
set allowed-interface-list=LAN
/tool mac-server mac-winbox
set allowed-interface-list=LAN
/tool netwatch
add comment="WANQUALITY probe lmt" disabled=no host=1.1.1.1 \
    ignore-initial-down=yes ignore-initial-up=yes interval=15m name=\
    WAN-quality-lmt-cf packet-count=5 packet-interval=200ms src-address=\
    192.168.0.215 start-delay=1s startup-delay=2m test-script=":local minRtt \
    \$\"rtt-min\"; :local avgRtt \$\"rtt-avg\"; :local maxRtt \$\"rtt-max\"; :\
    local jitter \$\"rtt-jitter\"; :local stdev \$\"rtt-stdev\"; :if ([:typeof\
    \_\$minRtt] = \"nil\") do={ :set minRtt 0ms }; :if ([:typeof \$avgRtt] = \
    \"nil\") do={ :set avgRtt 0ms }; :if ([:typeof \$maxRtt] = \"nil\") do={ :\
    set maxRtt 0ms }; :if ([:typeof \$jitter] = \"nil\") do={ :set jitter 0ms \
    }; :if ([:typeof \$stdev] = \"nil\") do={ :set stdev 0ms }; :log info (\"W\
    ANQUALITY type=icmp wan=lmt target=1.1.1.1 sent=\" . \$\"sent-count\" . \"\
    \_received=\" . \$\"response-count\" . \" loss=\" . \$\"loss-percent\" . \
    \" min=\" . \$minRtt . \" avg=\" . \$avgRtt . \" max=\" . \$maxRtt . \" ji\
    tter=\" . \$jitter . \" stdev=\" . \$stdev)" thr-avg=10s thr-jitter=10s \
    thr-loss-percent=100 thr-max=10s thr-stdev=10s type=icmp
add comment="WANQUALITY probe lmt" disabled=no host=8.8.8.8 \
    ignore-initial-down=yes ignore-initial-up=yes interval=15m name=\
    WAN-quality-lmt-google packet-count=5 packet-interval=200ms src-address=\
    192.168.0.215 start-delay=1s startup-delay=2m test-script=":local minRtt \
    \$\"rtt-min\"; :local avgRtt \$\"rtt-avg\"; :local maxRtt \$\"rtt-max\"; :\
    local jitter \$\"rtt-jitter\"; :local stdev \$\"rtt-stdev\"; :if ([:typeof\
    \_\$minRtt] = \"nil\") do={ :set minRtt 0ms }; :if ([:typeof \$avgRtt] = \
    \"nil\") do={ :set avgRtt 0ms }; :if ([:typeof \$maxRtt] = \"nil\") do={ :\
    set maxRtt 0ms }; :if ([:typeof \$jitter] = \"nil\") do={ :set jitter 0ms \
    }; :if ([:typeof \$stdev] = \"nil\") do={ :set stdev 0ms }; :log info (\"W\
    ANQUALITY type=icmp wan=lmt target=8.8.8.8 sent=\" . \$\"sent-count\" . \"\
    \_received=\" . \$\"response-count\" . \" loss=\" . \$\"loss-percent\" . \
    \" min=\" . \$minRtt . \" avg=\" . \$avgRtt . \" max=\" . \$maxRtt . \" ji\
    tter=\" . \$jitter . \" stdev=\" . \$stdev)" thr-avg=10s thr-jitter=10s \
    thr-loss-percent=100 thr-max=10s thr-stdev=10s type=icmp
add comment="WANQUALITY probe bite" disabled=no host=1.1.1.1 \
    ignore-initial-down=yes ignore-initial-up=yes interval=15m name=\
    WAN-quality-bite-cf packet-count=5 packet-interval=200ms src-address=\
    192.168.8.126 start-delay=1s startup-delay=2m test-script=":local minRtt \
    \$\"rtt-min\"; :local avgRtt \$\"rtt-avg\"; :local maxRtt \$\"rtt-max\"; :\
    local jitter \$\"rtt-jitter\"; :local stdev \$\"rtt-stdev\"; :if ([:typeof\
    \_\$minRtt] = \"nil\") do={ :set minRtt 0ms }; :if ([:typeof \$avgRtt] = \
    \"nil\") do={ :set avgRtt 0ms }; :if ([:typeof \$maxRtt] = \"nil\") do={ :\
    set maxRtt 0ms }; :if ([:typeof \$jitter] = \"nil\") do={ :set jitter 0ms \
    }; :if ([:typeof \$stdev] = \"nil\") do={ :set stdev 0ms }; :log info (\"W\
    ANQUALITY type=icmp wan=bite target=1.1.1.1 sent=\" . \$\"sent-count\" . \
    \" received=\" . \$\"response-count\" . \" loss=\" . \$\"loss-percent\" . \
    \" min=\" . \$minRtt . \" avg=\" . \$avgRtt . \" max=\" . \$maxRtt . \" ji\
    tter=\" . \$jitter . \" stdev=\" . \$stdev)" thr-avg=10s thr-jitter=10s \
    thr-loss-percent=100 thr-max=10s thr-stdev=10s type=icmp
add comment="WANQUALITY probe bite" disabled=no host=8.8.8.8 \
    ignore-initial-down=yes ignore-initial-up=yes interval=15m name=\
    WAN-quality-bite-google packet-count=5 packet-interval=200ms src-address=\
    192.168.8.126 start-delay=1s startup-delay=2m test-script=":local minRtt \
    \$\"rtt-min\"; :local avgRtt \$\"rtt-avg\"; :local maxRtt \$\"rtt-max\"; :\
    local jitter \$\"rtt-jitter\"; :local stdev \$\"rtt-stdev\"; :if ([:typeof\
    \_\$minRtt] = \"nil\") do={ :set minRtt 0ms }; :if ([:typeof \$avgRtt] = \
    \"nil\") do={ :set avgRtt 0ms }; :if ([:typeof \$maxRtt] = \"nil\") do={ :\
    set maxRtt 0ms }; :if ([:typeof \$jitter] = \"nil\") do={ :set jitter 0ms \
    }; :if ([:typeof \$stdev] = \"nil\") do={ :set stdev 0ms }; :log info (\"W\
    ANQUALITY type=icmp wan=bite target=8.8.8.8 sent=\" . \$\"sent-count\" . \
    \" received=\" . \$\"response-count\" . \" loss=\" . \$\"loss-percent\" . \
    \" min=\" . \$minRtt . \" avg=\" . \$avgRtt . \" max=\" . \$maxRtt . \" ji\
    tter=\" . \$jitter . \" stdev=\" . \$stdev)" thr-avg=10s thr-jitter=10s \
    thr-loss-percent=100 thr-max=10s thr-stdev=10s type=icmp
add comment="WANQUALITY probe lmt" host=1.1.1.1 interval=15m name=\
    WAN-quality-lmt-tcp port=443 src-address=192.168.0.215 start-delay=2s \
    startup-delay=5m test-script=":local connect \$\"tcp-connect-time\"; :if (\
    [:typeof \$connect] = \"nil\") do={ :set connect 0ms }; :log info (\"WANQU\
    ALITY type=tcp wan=lmt target=1.1.1.1:443 status=\" . \$status . \" connec\
    t=\" . \$connect)" timeout=5s type=tcp-conn
add comment="WANQUALITY probe lmt" dns-server=8.8.8.8 host=cloudflare.com \
    interval=15m name=WAN-quality-lmt-dns record-type=A src-address=\
    192.168.0.215 start-delay=4s startup-delay=5m test-script=":local answer \
    \$ip; :if ([:typeof \$answer] = \"nil\") do={ :set answer none }; :log inf\
    o (\"WANQUALITY type=dns wan=lmt target=cloudflare.com server=8.8.8.8 stat\
    us=\" . \$status . \" answer=\" . \$answer)" timeout=5s type=dns
add comment="WANQUALITY probe bite" host=1.1.1.1 interval=15m name=\
    WAN-quality-bite-tcp port=443 src-address=192.168.8.126 start-delay=2s \
    startup-delay=5m test-script=":local connect \$\"tcp-connect-time\"; :if (\
    [:typeof \$connect] = \"nil\") do={ :set connect 0ms }; :log info (\"WANQU\
    ALITY type=tcp wan=bite target=1.1.1.1:443 status=\" . \$status . \" conne\
    ct=\" . \$connect)" timeout=5s type=tcp-conn
add comment="WANQUALITY probe bite" dns-server=8.8.8.8 host=cloudflare.com \
    interval=15m name=WAN-quality-bite-dns record-type=A src-address=\
    192.168.8.126 start-delay=4s startup-delay=5m test-script=":local answer \
    \$ip; :if ([:typeof \$answer] = \"nil\") do={ :set answer none }; :log inf\
    o (\"WANQUALITY type=dns wan=bite target=cloudflare.com server=8.8.8.8 sta\
    tus=\" . \$status . \" answer=\" . \$answer)" timeout=5s type=dns
add comment="WANQUALITY probe lmt" dns-server=8.8.4.4 host=cloudflare.com \
    interval=15m name=WAN-quality-lmt-dns-secondary record-type=A \
    src-address=192.168.0.215 start-delay=4s startup-delay=5m test-script=":lo\
    cal answer \$ip; :if ([:typeof \$answer] = \"nil\") do={ :set answer none \
    }; :log info (\"WANQUALITY type=dns wan=lmt target=cloudflare.com server=8\
    .8.4.4 status=\" . \$status . \" answer=\" . \$answer)" timeout=5s type=\
    dns
add comment="WANQUALITY probe bite" dns-server=8.8.4.4 host=cloudflare.com \
    interval=15m name=WAN-quality-bite-dns-secondary record-type=A \
    src-address=192.168.8.126 start-delay=4s startup-delay=5m test-script=":lo\
    cal answer \$ip; :if ([:typeof \$answer] = \"nil\") do={ :set answer none \
    }; :log info (\"WANQUALITY type=dns wan=bite target=cloudflare.com server=\
    8.8.4.4 status=\" . \$status . \" answer=\" . \$answer)" timeout=5s type=\
    dns
