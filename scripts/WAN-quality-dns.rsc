# Keep DNS quality measurements aligned with the two configured RouterOS upstream resolvers.
:local lmtAddressWithMask [/ip dhcp-client get [find where name="client2"] address]
:local biteAddressWithMask [/ip dhcp-client get [find where name="client1"] address]
:local lmtAddress [:pick $lmtAddressWithMask 0 [:find $lmtAddressWithMask "/"]]
:local biteAddress [:pick $biteAddressWithMask 0 [:find $biteAddressWithMask "/"]]

/tool netwatch set [find where name="WAN-quality-lmt-dns"] dns-server=8.8.8.8 src-address=$lmtAddress test-script={
  :local answer $ip
  :if ([:typeof $answer] = "nil") do={ :set answer none }
  :log info ("WANQUALITY type=dns wan=lmt target=cloudflare.com server=8.8.8.8 status=" . $status . " answer=" . $answer)
}
/tool netwatch set [find where name="WAN-quality-bite-dns"] dns-server=8.8.8.8 src-address=$biteAddress test-script={
  :local answer $ip
  :if ([:typeof $answer] = "nil") do={ :set answer none }
  :log info ("WANQUALITY type=dns wan=bite target=cloudflare.com server=8.8.8.8 status=" . $status . " answer=" . $answer)
}

:if ([:len [/tool netwatch find where name="WAN-quality-lmt-dns-secondary"]] = 0) do={
  /tool netwatch add name="WAN-quality-lmt-dns-secondary" comment="WANQUALITY probe lmt" type=dns host=cloudflare.com record-type=A dns-server=8.8.4.4 src-address=$lmtAddress interval=15m timeout=5s start-delay=4s startup-delay=5m test-script={
    :local answer $ip
    :if ([:typeof $answer] = "nil") do={ :set answer none }
    :log info ("WANQUALITY type=dns wan=lmt target=cloudflare.com server=8.8.4.4 status=" . $status . " answer=" . $answer)
  }
} else={
  /tool netwatch set [find where name="WAN-quality-lmt-dns-secondary"] comment="WANQUALITY probe lmt" type=dns host=cloudflare.com record-type=A dns-server=8.8.4.4 src-address=$lmtAddress interval=15m timeout=5s start-delay=4s startup-delay=5m disabled=no test-script={
    :local answer $ip
    :if ([:typeof $answer] = "nil") do={ :set answer none }
    :log info ("WANQUALITY type=dns wan=lmt target=cloudflare.com server=8.8.4.4 status=" . $status . " answer=" . $answer)
  }
}

:if ([:len [/tool netwatch find where name="WAN-quality-bite-dns-secondary"]] = 0) do={
  /tool netwatch add name="WAN-quality-bite-dns-secondary" comment="WANQUALITY probe bite" type=dns host=cloudflare.com record-type=A dns-server=8.8.4.4 src-address=$biteAddress interval=15m timeout=5s start-delay=4s startup-delay=5m test-script={
    :local answer $ip
    :if ([:typeof $answer] = "nil") do={ :set answer none }
    :log info ("WANQUALITY type=dns wan=bite target=cloudflare.com server=8.8.4.4 status=" . $status . " answer=" . $answer)
  }
} else={
  /tool netwatch set [find where name="WAN-quality-bite-dns-secondary"] comment="WANQUALITY probe bite" type=dns host=cloudflare.com record-type=A dns-server=8.8.4.4 src-address=$biteAddress interval=15m timeout=5s start-delay=4s startup-delay=5m disabled=no test-script={
    :local answer $ip
    :if ([:typeof $answer] = "nil") do={ :set answer none }
    :log info ("WANQUALITY type=dns wan=bite target=cloudflare.com server=8.8.4.4 status=" . $status . " answer=" . $answer)
  }
}
