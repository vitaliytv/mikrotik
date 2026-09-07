# Restore the pre-AQM WAN interface queues without changing routes or failover state.
:local lmtQueue [/queue interface find where interface="ether3"]
:local biteQueue [/queue interface find where interface="ether1"]
:if (([:len $lmtQueue] != 1) || ([:len $biteQueue] != 1)) do={ :error "WAN-AQM rollback interface queue missing or duplicated" }
/queue interface set $lmtQueue queue=only-hardware-queue
/queue interface set $biteQueue queue=only-hardware-queue
