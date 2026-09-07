# Write one combined, minute-level decision snapshot for both WANs to wan-quality disk history.
:if ([/system script job print count-only as-value where script=[:jobname]] > 1) do={ :error "DUALWAN-quality-log is already running" }

:global dwActiveBad
:global dwActiveState
:global dwQualityBad
:global dwSevereBad
:global dwLastSwitchUptime

:local metric do={
  :local probeName $1
  :local property $2
  :local ids [/tool netwatch find where name=$probeName]
  :if ([:len $ids] != 1) do={ :return "none" }
  :local value [/tool netwatch get $ids $property]
  :if (([:typeof $value] = "nil") || ([:typeof $value] = "nothing")) do={ :return "none" }
  :return $value
}

:local lmtReceived [$metric "DUALWAN-quality-lmt-icmp" "response-count"]
:local lmtAvg [$metric "DUALWAN-quality-lmt-icmp" "rtt-avg"]
:local lmtMax [$metric "DUALWAN-quality-lmt-icmp" "rtt-max"]
:local lmtJitter [$metric "DUALWAN-quality-lmt-icmp" "rtt-jitter"]
:local lmtTcpStatus [$metric "DUALWAN-quality-lmt-tcp" "status"]
:local lmtTcp [$metric "DUALWAN-quality-lmt-tcp" "tcp-connect-time"]
:local biteReceived [$metric "DUALWAN-quality-bite-icmp" "response-count"]
:local biteAvg [$metric "DUALWAN-quality-bite-icmp" "rtt-avg"]
:local biteMax [$metric "DUALWAN-quality-bite-icmp" "rtt-max"]
:local biteJitter [$metric "DUALWAN-quality-bite-icmp" "rtt-jitter"]
:local biteTcpStatus [$metric "DUALWAN-quality-bite-tcp" "status"]
:local biteTcp [$metric "DUALWAN-quality-bite-tcp" "tcp-connect-time"]

:local uplink do={
  :local interfaceName $1
  :global dwQualityTxBps
  :set dwQualityTxBps 0
  /interface monitor-traffic $interfaceName once do={ :global dwQualityTxBps $"tx-bits-per-second" }
  :return $dwQualityTxBps
}
:local aqm do={
  :local interfaceName $1
  :local queueId [/queue interface find where interface=$interfaceName]
  :if ([:len $queueId] != 1) do={ :return {"none";0} }
  :local queueName [/queue interface get $queueId queue]
  :local typeId [/queue type find where name=$queueName]
  :if ([:len $typeId] != 1) do={ :return {$queueName;0} }
  :local bandwidth [/queue type get $typeId cake-bandwidth]
  :if ([:typeof $bandwidth] != "num") do={ :set bandwidth 0 }
  :return {$queueName;$bandwidth}
}

:local lmtTxBps [$uplink "ether3"]
:local biteTxBps [$uplink "ether1"]
:local lmtAqm [$aqm "ether3"]
:local biteAqm [$aqm "ether1"]

:local lmtTables [/ip dhcp-client get [find where name="client2"] default-route-tables]
:local active "bite"
:if ([:typeof [:find $lmtTables "main:1"]] != "nil") do={ :set active "lmt" }
:local hardBad 0
:local qualityBad 0
:local severeBad 0
:local lastSwitch 0ms
:if ([:typeof $dwActiveBad] = "num") do={ :set hardBad $dwActiveBad }
:if ([:typeof $dwQualityBad] = "num") do={ :set qualityBad $dwQualityBad }
:if ([:typeof $dwSevereBad] = "num") do={ :set severeBad $dwSevereBad }
:if ([:typeof $dwLastSwitchUptime] = "time") do={ :set lastSwitch $dwLastSwitchUptime }

:log info ("WANQUALITY type=decision active=" . $active . " hard_bad=" . $hardBad . " severe_bad=" . $severeBad . " quality_bad=" . $qualityBad . " last_switch=" . $lastSwitch . " lmt_sent=3 lmt_received=" . $lmtReceived . " lmt_avg=" . $lmtAvg . " lmt_max=" . $lmtMax . " lmt_jitter=" . $lmtJitter . " lmt_tcp_status=" . $lmtTcpStatus . " lmt_tcp=" . $lmtTcp . " lmt_tx_bps=" . $lmtTxBps . " lmt_aqm=" . ($lmtAqm->0) . " lmt_capacity_bps=" . ($lmtAqm->1) . " bite_sent=3 bite_received=" . $biteReceived . " bite_avg=" . $biteAvg . " bite_max=" . $biteMax . " bite_jitter=" . $biteJitter . " bite_tcp_status=" . $biteTcpStatus . " bite_tcp=" . $biteTcp . " bite_tx_bps=" . $biteTxBps . " bite_aqm=" . ($biteAqm->0) . " bite_capacity_bps=" . ($biteAqm->1))
