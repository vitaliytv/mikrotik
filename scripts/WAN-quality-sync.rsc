# Keep monitor-only Netwatch probes pinned to the current DHCP address of each WAN.
:if ([/system script job print count-only as-value where script=[:jobname]] > 1) do={ :error "WAN-quality-sync is already running" }

:local syncWan do={
  :local wan $1
  :local clientName $2
  :local tableName $3
  :local clientId [/ip dhcp-client find where name=$clientName]
  :if ([:len $clientId] != 1) do={ :error ("WANQUALITY sync missing DHCP client=" . $clientName) }
  :local addressWithMask [/ip dhcp-client get $clientId address]
  :local slash [:find $addressWithMask "/"]
  :if ([:typeof $slash] = "nil") do={ :error ("WANQUALITY sync missing address wan=" . $wan) }
  :local address [:pick $addressWithMask 0 $slash]

  :local ruleComment ("WANQUALITY route " . $wan)
  :local ruleIds [/routing rule find where comment=$ruleComment]
  :if ([:len $ruleIds] != 1) do={ :error ("WANQUALITY sync expected one route rule wan=" . $wan) }
  /routing rule set $ruleIds src-address=($address . "/32") action=lookup-only-in-table table=$tableName

  :local probeComment ("WANQUALITY probe " . $wan)
  :local probeIds [/tool netwatch find where comment=$probeComment]
  :if ([:len $probeIds] != 5) do={ :error ("WANQUALITY sync expected five probes wan=" . $wan) }
  /tool netwatch set $probeIds src-address=$address
}

$syncWan "lmt" "client2" "to_WAN1"
$syncWan "bite" "client1" "to_WAN2"
