# Keep common certificate-status hostnames warm in the RouterOS DNS cache.
:if ([/system script job print count-only as-value where script=[:jobname]] > 1) do={ :error "DNS cache warmer is already running" }

:local names {"ocsp.digicert.com"; "ocsp.sectigo.com"; "ocsp.globalsign.com"; "crl3.digicert.com"}
:local failed 0
:local maximumMs 0

:foreach name in=$names do={
  :local started [:timestamp]
  :local answer ""
  :local errorText ""
  :onerror error in={ :set answer [:resolve $name] } do={ :set errorText $error }
  :local elapsed ([:timestamp] - $started)
  :local elapsedMs ($elapsed / 1ms)
  :if ($elapsedMs > $maximumMs) do={ :set maximumMs $elapsedMs }
  :if ([:len $errorText] > 0) do={
    :set failed ($failed + 1)
  }
}

# Successful warm and refresh passes stay silent; only failures reach disk.
:if ($failed > 0) do={
  :log info ("WANQUALITY type=dns-cache-warm names=" . [:len $names] . " failed=" . $failed . " max_ms=" . $maximumMs)
}
