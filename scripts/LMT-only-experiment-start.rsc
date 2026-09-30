# Enter a reversible LMT-only experiment while preserving LMT disk telemetry.
:if ([/system script job print count-only as-value where script=[:jobname]] > 1) do={ :error "LMT-only experiment start is already running" }

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
:if ([/ip dhcp-client get $lmtClient disabled]) do={ :error "LMTONLY LMT client is disabled" }
:if ([/ip dhcp-client get $lmtClient status] != "bound") do={ :error "LMTONLY LMT client is not bound" }
:local lmtTables [/ip dhcp-client get $lmtClient default-route-tables]
:if ([:typeof [:find $lmtTables "main:1"]] = "nil") do={ :error "LMTONLY LMT is not main:1" }

:local dnsServers [/ip dns get value-name=servers]
:local alreadyActive (([/system scheduler get $healthScheduler disabled]) && ([/ip dhcp-client get $biteClient disabled]) && ($dnsServers = "192.168.0.1"))
:if ($alreadyActive) do={
  :log info "LMTONLY state=start result=already-active"
} else={
  # Stop all switching and combined high-frequency checks before removing BITE.
  /system scheduler set $healthScheduler disabled=yes
  /system scheduler set $qualityLogScheduler disabled=yes
  /tool netwatch set $decisionProbes disabled=yes
  /tool netwatch set $biteProbes disabled=yes

  # Avoid a two-second lookup delay against the BITE modem after its lease is gone.
  /ip dns set servers=192.168.0.1
  /ip dhcp-client set $biteClient disabled=yes

  # LMT monitor-only probes and their source-address maintenance remain active.
  /tool netwatch set $lmtProbes disabled=no
  /system scheduler set $syncScheduler disabled=no
  /system script run WAN-quality-sync

  :log warning ("LMTONLY state=start mode=manual-stop date=" . [/system clock get date] . " time=" . [/system clock get time] . " dns=192.168.0.1")
}
