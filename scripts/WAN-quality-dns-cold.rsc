# Measure a deliberately uncached recursive DNS lookup through the RouterOS resolver.
:if ([/system script job print count-only as-value where script=[:jobname]] > 1) do={ :error "WAN quality cold DNS probe is already running" }

:local activeWan "unknown"
:local lmtClient [/ip dhcp-client find where name="client2"]
:local biteClient [/ip dhcp-client find where name="client1"]
:if (([:len $lmtClient] = 1) && (![/ip dhcp-client get $lmtClient disabled]) && ([/ip dhcp-client get $lmtClient status] = "bound")) do={
  :local lmtTables [/ip dhcp-client get $lmtClient default-route-tables]
  :if ([:typeof [:find $lmtTables "main:1"]] != "nil") do={ :set activeWan "lmt" }
}
:if (([:len $biteClient] = 1) && (![/ip dhcp-client get $biteClient disabled]) && ([/ip dhcp-client get $biteClient status] = "bound")) do={
  :local biteTables [/ip dhcp-client get $biteClient default-route-tables]
  :if ([:typeof [:find $biteTables "main:1"]] != "nil") do={ :set activeWan "bite" }
}

:local query ([:rndstr length=12 from="abcdefghijklmnopqrstuvwxyz0123456789"] . ".1-1-1-1.sslip.io")
:local started [:timestamp]
:local answer ""
:local errorText ""
:onerror error in={ :set answer [:resolve $query] } do={ :set errorText $error }
:local elapsed ([:timestamp] - $started)
:local elapsedMs ($elapsed / 1ms)
:local status "up"

# sslip.io returns the embedded IPv4 address for every unique child name.
:if ([:len $errorText] > 0) do={ :set status "error" }
:if (([:len $errorText] = 0) && ($answer != "1.1.1.1")) do={ :set status "unexpected-answer" }

:log info ("WANQUALITY type=dns-cold wan=" . $activeWan . " resolver=router status=" . $status . " duration_ms=" . $elapsedMs . " answer=" . $answer . " query=" . $query)
