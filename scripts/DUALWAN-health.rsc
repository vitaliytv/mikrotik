# Exactly one RouterOS failover controller. Both WANs use the same health rule.
:if ([/system script job print count-only as-value where script=[:jobname]] > 1) do={ :error "DUALWAN-health is already running" }

:global dwActiveBad
:global dwActiveState
:if ([:typeof $dwActiveBad] = "nothing") do={ :set dwActiveBad 0 }

# Initialize once after reboot from the installed main-route priority.
:if ([:typeof $dwActiveState] = "nothing") do={
  :set dwActiveState "bite"
  :local lmtTables [/ip dhcp-client get [find where name="client2"] default-route-tables]
  :if (([:len $lmtTables] >= 6) && ([:pick $lmtTables 0 6] = "main:1")) do={ :set dwActiveState "lmt" }
}

:local current $dwActiveState
:local activeInterface "ether1"
:if ($current = "lmt") do={ :set activeInterface "ether3" }
:local edgeReceived [/ping address=212.93.105.242 interface=$activeInterface count=3 interval=200ms]
:local publicReceived [/ping address=1.1.1.1 interface=$activeInterface count=3 interval=200ms]
:local activeGood (($edgeReceived >= 2) || ($publicReceived >= 2))
:local next $current
:local reason "active-healthy"

:if ($activeGood) do={
  :set dwActiveBad 0
} else={
  :set dwActiveBad ($dwActiveBad + 1)
  :set reason "active-probes-degraded-keep-current"
  :if ($dwActiveBad >= 3) do={
    :if ($current = "lmt") do={ :set next "bite" } else={ :set next "lmt" }
    :set dwActiveBad 0
    :set reason "active-probes-degraded-3x-switch-next"
  }
}

:if ($next != $current) do={
  :if ($next = "bite") do={
    /ip dhcp-client set [find where name="client1"] default-route-tables="main:1,to_WAN1:1,to_WAN2:1"
    :delay 1s
    /ip dhcp-client set [find where name="client2"] default-route-tables="main:2,to_WAN1:2,to_WAN2:2"
  } else={
    /ip dhcp-client set [find where name="client2"] default-route-tables="main:1,to_WAN1:1,to_WAN2:2"
    :delay 1s
    /ip dhcp-client set [find where name="client1"] default-route-tables="main:2,to_WAN1:2,to_WAN2:1"
  }
  :set dwActiveState $next
  :log warning ("DUALWAN state=" . $next . " from=" . $current . " reason=" . $reason . " interface=" . $activeInterface . " edge-received=" . $edgeReceived . "/3 public-received=" . $publicReceived . "/3")
}
