# Exactly one RouterOS failover controller. All decisions use source-routed Netwatch results.
:if ([/system script job print count-only as-value where script=[:jobname]] > 1) do={ :error "DUALWAN-health is already running" }

:global dwActiveBad
:global dwActiveState
:global dwQualityBad
:global dwSevereBad
:global dwLmtDone
:global dwBiteDone
:global dwLastSwitchUptime
:if ([:typeof $dwActiveBad] != "num") do={ :set dwActiveBad 0 }
:if ([:typeof $dwQualityBad] != "num") do={ :set dwQualityBad 0 }
:if ([:typeof $dwSevereBad] != "num") do={ :set dwSevereBad 0 }
:if ([:typeof $dwLmtDone] != "num") do={ :set dwLmtDone 0 }
:if ([:typeof $dwBiteDone] != "num") do={ :set dwBiteDone 0 }
:if ([:typeof $dwLastSwitchUptime] != "time") do={ :set dwLastSwitchUptime 0ms }

# DHCP route priorities are authoritative; globals only preserve streak state.
:local lmtTables [/ip dhcp-client get [find where name="client2"] default-route-tables]
:local current "bite"
:if ([:typeof [:find $lmtTables "main:1"]] != "nil") do={ :set current "lmt" }
:if (([:typeof $dwActiveState] != "str") || ($dwActiveState != $current)) do={
  :set dwActiveBad 0
  :set dwQualityBad 0
  :set dwSevereBad 0
}
:set dwActiveState $current

:local activeQualityName "DUALWAN-quality-bite-icmp"
:local activeTcpName "DUALWAN-quality-bite-tcp"
:local candidateQualityName "DUALWAN-quality-lmt-icmp"
:local candidateTcpName "DUALWAN-quality-lmt-tcp"
:local lastActiveDone $dwBiteDone
:local next "lmt"
:if ($current = "lmt") do={
  :set activeQualityName "DUALWAN-quality-lmt-icmp"
  :set activeTcpName "DUALWAN-quality-lmt-tcp"
  :set candidateQualityName "DUALWAN-quality-bite-icmp"
  :set candidateTcpName "DUALWAN-quality-bite-tcp"
  :set lastActiveDone $dwLmtDone
  :set next "bite"
}

:local activeQualityId [/tool netwatch find where name=$activeQualityName]
:local activeTcpId [/tool netwatch find where name=$activeTcpName]
:local candidateQualityId [/tool netwatch find where name=$candidateQualityName]
:local candidateTcpId [/tool netwatch find where name=$candidateTcpName]
:if (([:len $activeQualityId] != 1) || ([:len $activeTcpId] != 1) || ([:len $candidateQualityId] != 1) || ([:len $candidateTcpId] != 1)) do={
  :error "DUALWAN quality probes are missing or duplicated"
}

:local activeDone [/tool netwatch get $activeQualityId done-tests]
:local activeReceived [/tool netwatch get $activeQualityId response-count]
:local activeAvg [/tool netwatch get $activeQualityId rtt-avg]
:local activeMax [/tool netwatch get $activeQualityId rtt-max]
:local activeJitter [/tool netwatch get $activeQualityId rtt-jitter]
:local activeTcpStatus [/tool netwatch get $activeTcpId status]
:local activeTcp [/tool netwatch get $activeTcpId tcp-connect-time]
:local candidateReceived [/tool netwatch get $candidateQualityId response-count]
:local candidateAvg [/tool netwatch get $candidateQualityId rtt-avg]
:local candidateMax [/tool netwatch get $candidateQualityId rtt-max]
:local candidateJitter [/tool netwatch get $candidateQualityId rtt-jitter]
:local candidateTcpStatus [/tool netwatch get $candidateTcpId status]
:local candidateTcp [/tool netwatch get $candidateTcpId tcp-connect-time]

:local metricsReady (([:typeof $activeAvg] = "time") && ([:typeof $activeMax] = "time") && ([:typeof $activeJitter] = "time") && ([:typeof $activeTcp] = "time") && ([:typeof $candidateAvg] = "time") && ([:typeof $candidateMax] = "time") && ([:typeof $candidateJitter] = "time") && ([:typeof $candidateTcp] = "time"))
:local activeHardBad (($activeReceived < 2) && ($activeTcpStatus != "up"))
:local candidateAvailable (($candidateReceived >= 2) || ($candidateTcpStatus = "up"))
:local activeQualityBad false
:local activeSevereBad false
:local candidateQualityGood false
:local candidateBetterSoft false
:local candidateBetterSevere false
:if ($metricsReady) do={
  :set activeQualityBad (($activeReceived < 3) || ($activeTcpStatus != "up") || ($activeAvg > 80ms) || ($activeMax > 180ms) || ($activeJitter > 120ms) || ($activeTcp > 300ms))
  :set activeSevereBad (($activeAvg > 150ms) || ($activeMax > 300ms) || ($activeJitter > 250ms) || ($activeTcp > 400ms))
  :set candidateQualityGood (($candidateReceived >= 3) && ($candidateTcpStatus = "up") && ($candidateAvg <= 80ms) && ($candidateMax <= 160ms) && ($candidateJitter <= 100ms) && ($candidateTcp <= 250ms))
  :set candidateBetterSoft (($candidateAvg + 40ms) < $activeAvg)
  :set candidateBetterSevere (($candidateAvg + 20ms) < $activeAvg)
}

# A streak advances only on a fresh source-routed ICMP sample from the active WAN.
:if (($activeDone != $lastActiveDone) && $metricsReady) do={
  :if ($current = "lmt") do={ :set dwLmtDone $activeDone } else={ :set dwBiteDone $activeDone }
  :if ($activeHardBad) do={ :set dwActiveBad ($dwActiveBad + 1) } else={ :set dwActiveBad 0 }
  :if ($activeSevereBad && $candidateQualityGood && $candidateBetterSevere) do={
    :set dwSevereBad ($dwSevereBad + 1)
  } else={
    :set dwSevereBad 0
  }
  :if ($activeQualityBad && $candidateQualityGood && $candidateBetterSoft) do={
    :set dwQualityBad ($dwQualityBad + 1)
  } else={
    :set dwQualityBad 0
  }
}

:local nowUptime [/system resource get uptime]
:local severeHoldExpired (($nowUptime - $dwLastSwitchUptime) >= 5m)
:local softHoldExpired (($nowUptime - $dwLastSwitchUptime) >= 15m)
:local shouldSwitch false
:local reason "active-healthy"
:if (($dwActiveBad >= 3) && $candidateAvailable) do={
  :set shouldSwitch true
  :set reason "active-source-probes-down-3x-candidate-up"
} else={
  :if (($dwSevereBad >= 3) && $severeHoldExpired) do={
    :set shouldSwitch true
    :set reason "active-quality-severe-3x-candidate-better"
  } else={
    :if (($dwQualityBad >= 6) && $softHoldExpired) do={
      :set shouldSwitch true
      :set reason "active-quality-soft-6x-candidate-better"
    }
  }
}

:if ($shouldSwitch) do={
  :if ($next = "bite") do={
    # Policy tables stay pinned: to_WAN1=LMT, to_WAN2=BITE. Only main changes priority.
    /ip dhcp-client set [find where name="client1"] default-route-tables="main:1,to_WAN2:1"
    :delay 1s
    /ip dhcp-client set [find where name="client2"] default-route-tables="main:2,to_WAN1:1"
  } else={
    /ip dhcp-client set [find where name="client2"] default-route-tables="main:1,to_WAN1:1"
    :delay 1s
    /ip dhcp-client set [find where name="client1"] default-route-tables="main:2,to_WAN2:1"
  }
  :set dwActiveState $next
  :set dwActiveBad 0
  :set dwQualityBad 0
  :set dwSevereBad 0
  :set dwLastSwitchUptime $nowUptime
  :log warning ("DUALWAN state=" . $next . " from=" . $current . " reason=" . $reason . " active-icmp=" . $activeReceived . "/3 active-avg=" . $activeAvg . " active-max=" . $activeMax . " active-jitter=" . $activeJitter . " active-tcp=" . $activeTcpStatus . "/" . $activeTcp . " candidate-icmp=" . $candidateReceived . "/3 candidate-avg=" . $candidateAvg . " candidate-tcp=" . $candidateTcpStatus . "/" . $candidateTcp)
}
