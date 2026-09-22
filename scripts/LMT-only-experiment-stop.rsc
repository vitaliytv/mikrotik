# Restore dual-WAN manually after the LMT-only experiment.
:if ([/system script job print count-only as-value where script=[:jobname]] > 1) do={ :error "LMT-only experiment stop is already running" }

:local healthScheduler [/system scheduler find where name="DUALWAN-health-every-5s"]
:local qualityLogScheduler [/system scheduler find where name="DUALWAN-quality-log-every-1m"]
:local syncScheduler [/system scheduler find where name="WAN-quality-sync-every-5m"]
:local lmtClient [/ip dhcp-client find where name="client2"]
:local biteClient [/ip dhcp-client find where name="client1"]
:local lmtProbes [/tool netwatch find where comment="WANQUALITY probe lmt"]
:local biteProbes [/tool netwatch find where comment="WANQUALITY probe bite"]
:local decisionProbes [/tool netwatch find where comment="DUALWAN decision probe"]

:if (([:len $healthScheduler] != 1) || ([:len $qualityLogScheduler] != 1) || ([:len $syncScheduler] != 1)) do={ :error "LMTONLY expected unique schedulers" }
:if (([:len $lmtClient] != 1) || ([:len $biteClient] != 1)) do={ :error "LMTONLY expected unique DHCP clients" }
:if (([:len $lmtProbes] != 5) || ([:len $biteProbes] != 5) || ([:len $decisionProbes] != 4)) do={ :error "LMTONLY expected 5 LMT, 5 BITE and 4 decision probes" }
:if ([/ip dhcp-client get $lmtClient status] != "bound") do={ :error "LMTONLY LMT client is not bound; refusing dual-WAN restore" }

# Keep the controller stopped until BITE is actually ready.
/system scheduler set $healthScheduler disabled=yes
/system scheduler set $qualityLogScheduler disabled=yes
/tool netwatch set $decisionProbes disabled=yes
/ip dhcp-client set $biteClient disabled=no

:local attempts 0
:while (($attempts < 30) && ([/ip dhcp-client get $biteClient status] != "bound")) do={
  :delay 1s
  :set attempts ($attempts + 1)
}
:if ([/ip dhcp-client get $biteClient status] != "bound") do={
  /ip dns set servers=192.168.0.1
  :log error "LMTONLY state=stop result=bite-not-bound lmt-remains-active"
  :error "LMTONLY BITE did not bind; controller remains disabled"
}

/system scheduler set $syncScheduler disabled=no
/system script run WAN-quality-sync
/tool netwatch set $lmtProbes disabled=no
/tool netwatch set $biteProbes disabled=no
/tool netwatch set $decisionProbes disabled=no
:delay 6s

/ip dns set servers=192.168.8.1,192.168.0.1
/system scheduler set $qualityLogScheduler disabled=no
/system scheduler set $healthScheduler disabled=no

:log warning ("LMTONLY state=stop result=dual-wan-restored date=" . [/system clock get date] . " time=" . [/system clock get time])
