# Persist passive physical-interface counters without generating network traffic.
:if ([/system script job print count-only as-value where script=[:jobname]] > 1) do={ :error "WAN-quality-interface is already running" }

:local logInterface do={
  :local wan $1
  :local interfaceName $2
  :local interfaceId [/interface find where name=$interfaceName]
  :local ethernetId [/interface ethernet find where name=$interfaceName]
  :if ([:len $interfaceId] != 1) do={ :error ("WANQUALITY interface missing interface=" . $interfaceName) }
  :if ([:len $ethernetId] != 1) do={ :error ("WANQUALITY interface missing ethernet=" . $interfaceName) }

  :local running [/interface get $interfaceId running]
  :local queueDrops [/interface get $interfaceId tx-queue-drop]
  :local linkDowns [/interface get $interfaceId link-downs]
  :local fcsErrors [/interface ethernet get $ethernetId rx-fcs-error]
  :local alignErrors [/interface ethernet get $ethernetId rx-align-error]
  :local collisions [/interface ethernet get $ethernetId tx-collision]
  :log info ("WANQUALITY type=interface wan=" . $wan . " interface=" . $interfaceName . " running=" . $running . " tx_queue_drop=" . $queueDrops . " link_downs=" . $linkDowns . " rx_fcs_error=" . $fcsErrors . " rx_align_error=" . $alignErrors . " tx_collision=" . $collisions)
}

$logInterface "lmt" "ether3"
$logInterface "bite" "ether1"
