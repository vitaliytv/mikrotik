# Install silent, source-routed quality probes used only by DUALWAN-health decisions.
:local ensureIcmp do={
  :local probeName $1
  :local sourceAddress $2
  :local ids [/tool netwatch find where name=$probeName]
  :if ([:len $ids] = 0) do={
    /tool netwatch add name=$probeName comment="DUALWAN decision probe" type=icmp host=1.1.1.1 src-address=$sourceAddress interval=5s packet-count=3 packet-interval=200ms thr-loss-percent=100 thr-avg=10s thr-max=10s thr-jitter=10s thr-stdev=10s start-delay=1s startup-delay=1s ignore-initial-up=yes ignore-initial-down=yes
  } else={
    :if ([:len $ids] != 1) do={ :error ("DUALWAN duplicate probe=" . $probeName) }
    /tool netwatch set $ids comment="DUALWAN decision probe" type=icmp host=1.1.1.1 src-address=$sourceAddress interval=5s packet-count=3 packet-interval=200ms thr-loss-percent=100 thr-avg=10s thr-max=10s thr-jitter=10s thr-stdev=10s start-delay=1s startup-delay=1s ignore-initial-up=yes ignore-initial-down=yes disabled=no test-script="" up-script="" down-script=""
  }
}

:local ensureTcp do={
  :local probeName $1
  :local sourceAddress $2
  :local ids [/tool netwatch find where name=$probeName]
  :if ([:len $ids] = 0) do={
    /tool netwatch add name=$probeName comment="DUALWAN decision probe" type=tcp-conn host=1.1.1.1 port=443 src-address=$sourceAddress interval=5s timeout=2s start-delay=2s startup-delay=1s ignore-initial-up=yes ignore-initial-down=yes
  } else={
    :if ([:len $ids] != 1) do={ :error ("DUALWAN duplicate probe=" . $probeName) }
    /tool netwatch set $ids comment="DUALWAN decision probe" type=tcp-conn host=1.1.1.1 port=443 src-address=$sourceAddress interval=5s timeout=2s start-delay=2s startup-delay=1s ignore-initial-up=yes ignore-initial-down=yes disabled=no test-script="" up-script="" down-script=""
  }
}

:local clientAddress do={
  :local clientId [/ip dhcp-client find where name=$1]
  :if ([:len $clientId] != 1) do={ :error ("DUALWAN missing DHCP client=" . $1) }
  :local addressWithMask [/ip dhcp-client get $clientId address]
  :local slash [:find $addressWithMask "/"]
  :if ([:typeof $slash] = "nil") do={ :error ("DUALWAN DHCP client has no address=" . $1) }
  :return [:pick $addressWithMask 0 $slash]
}

:local lmtAddress [$clientAddress "client2"]
:local biteAddress [$clientAddress "client1"]
$ensureIcmp "DUALWAN-quality-lmt-icmp" $lmtAddress
$ensureTcp "DUALWAN-quality-lmt-tcp" $lmtAddress
$ensureIcmp "DUALWAN-quality-bite-icmp" $biteAddress
$ensureTcp "DUALWAN-quality-bite-tcp" $biteAddress
