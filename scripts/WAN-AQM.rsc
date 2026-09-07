# Shape each cellular uplink below its conservative observed capacity so CAKE owns the bottleneck.
:local ensureCake do={
  :local queueName $1
  :local bandwidth $2
  :local ids [/queue type find where name=$queueName]
  :if ([:len $ids] = 0) do={
    /queue type add name=$queueName kind=cake cake-bandwidth=$bandwidth cake-diffserv=diffserv3 cake-flowmode=triple-isolate cake-nat=yes cake-rtt=100ms cake-ack-filter=none
  } else={
    :if ([:len $ids] != 1) do={ :error ("WAN-AQM queue type duplicated: " . $queueName) }
    /queue type set $ids kind=cake cake-bandwidth=$bandwidth cake-diffserv=diffserv3 cake-flowmode=triple-isolate cake-nat=yes cake-rtt=100ms cake-ack-filter=none
  }
}

$ensureCake "cake-wan-lmt" 7M
$ensureCake "cake-wan-bite" 10M

:local lmtQueue [/queue interface find where interface="ether3"]
:local biteQueue [/queue interface find where interface="ether1"]
:if (([:len $lmtQueue] != 1) || ([:len $biteQueue] != 1)) do={ :error "WAN-AQM interface queue missing or duplicated" }
/queue interface set $lmtQueue queue=cake-wan-lmt
/queue interface set $biteQueue queue=cake-wan-bite
/queue simple disable [find where name="QoS-up"]
