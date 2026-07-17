#!/bin/bash

# Remove obsolete Google Cloud resources from efes-md while preserving
# Geocoding API, Dynamic Maps API, and the protected Maps API key.

set -u
set -o pipefail

PROJECT_ID="efes-md"
REGION="europe-central2"
ZONE="europe-central2-b"
PROTECTED_KEY_ID="498ca162-ecb9-4fd4-b5e9-1e452026fcdd"
PROTECTED_SERVICES="geocoding-backend.googleapis.com maps-backend.googleapis.com"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
LOG_DIR="${SCRIPT_DIR}/logs"
TIMESTAMP="$(date '+%Y%m%d-%H%M%S')"
LOG_FILE="${LOG_DIR}/cleanup-efes-md-${TIMESTAMP}.log"
MODE=""
ASSUME_YES=0
FAILURES=0

export CLOUDSDK_CORE_DISABLE_PROMPTS=1

usage() {
  cat <<'EOF'
Usage:
  ./scripts/cleanup_efes_md.sh --diagnose
  ./scripts/cleanup_efes_md.sh --execute
  ./scripts/cleanup_efes_md.sh --execute --yes

Modes:
  --diagnose  Read-only inventory. Makes no cloud changes.
  --execute   Run the approved cleanup after an interactive confirmation.
  --yes       Skip the interactive confirmation. Only valid with --execute.

Every run writes a log under scripts/logs/.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --diagnose)
      MODE="diagnose"
      ;;
    --execute)
      MODE="execute"
      ;;
    --yes)
      ASSUME_YES=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

if [ -z "${MODE}" ]; then
  usage >&2
  exit 2
fi

if [ "${MODE}" = "diagnose" ] && [ "${ASSUME_YES}" -eq 1 ]; then
  echo "--yes is only valid with --execute" >&2
  exit 2
fi

mkdir -p "${LOG_DIR}"
exec > >(tee -a "${LOG_FILE}") 2>&1

section() {
  echo
  echo "================================================================"
  echo "$1"
  echo "================================================================"
}

print_command() {
  printf '$'
  printf ' %q' "$@"
  printf '\n'
}

run() {
  echo
  print_command "$@"
  "$@"
  local status=$?
  if [ "${status}" -ne 0 ]; then
    echo "ERROR: exit ${status}"
    FAILURES=$((FAILURES + 1))
  fi
  return 0
}

run_required() {
  echo
  print_command "$@"
  "$@"
  local status=$?
  if [ "${status}" -ne 0 ]; then
    echo "FATAL: exit ${status}"
    echo "No destructive commands were started."
    exit "${status}"
  fi
}

preflight() {
  section "Preflight"

  if ! command -v gcloud >/dev/null 2>&1; then
    echo "FATAL: gcloud is not installed or is not in PATH."
    exit 127
  fi

  run_required gcloud auth list \
    --filter=status:ACTIVE \
    --format=value\(account\)

  local actual_project
  actual_project="$(gcloud projects describe "${PROJECT_ID}" --format=value\(projectId\) 2>/dev/null)"
  if [ "${actual_project}" != "${PROJECT_ID}" ]; then
    echo "FATAL: expected project ${PROJECT_ID}, got '${actual_project}'."
    exit 1
  fi
  echo "Target project verified: ${actual_project}"

  run_required gcloud services api-keys describe "${PROTECTED_KEY_ID}" \
    --project="${PROJECT_ID}" \
    --format=yaml\(displayName\,restrictions\)

  local enabled_services
  enabled_services="$(gcloud services list --enabled --project="${PROJECT_ID}" --format=value\(config.name\))"
  local protected_service
  for protected_service in ${PROTECTED_SERVICES}; do
    if ! printf '%s\n' "${enabled_services}" | grep -qx "${protected_service}"; then
      echo "FATAL: protected service is not enabled: ${protected_service}"
      exit 1
    fi
    echo "Protected service enabled: ${protected_service}"
  done
}

diagnose() {
  section "Cloud Asset summary"
  run gcloud asset search-all-resources \
    --scope="projects/${PROJECT_ID}" \
    --billing-project="${PROJECT_ID}" \
    --limit=1000 \
    --format=table\(assetType.basename\(\)\,displayName\,location\,state\)

  section "Compute and network"
  run gcloud compute instances list --project="${PROJECT_ID}"
  run gcloud compute disks list --project="${PROJECT_ID}"
  run gcloud compute snapshots list --project="${PROJECT_ID}"
  run gcloud compute addresses list --project="${PROJECT_ID}"
  run gcloud compute routers list --project="${PROJECT_ID}"
  run gcloud compute forwarding-rules list --project="${PROJECT_ID}"
  run gcloud container clusters list --project="${PROJECT_ID}"

  section "Storage and registry"
  run gcloud storage buckets list --project="${PROJECT_ID}"
  run gcloud artifacts repositories list --project="${PROJECT_ID}" --location=all

  section "Identity and APIs"
  run gcloud services api-keys list --project="${PROJECT_ID}"
  run gcloud secrets list --project="${PROJECT_ID}"
  run gcloud iam service-accounts list --project="${PROJECT_ID}"
  run gcloud services list --enabled --project="${PROJECT_ID}"
}

confirm_cleanup() {
  if [ "${ASSUME_YES}" -eq 1 ]; then
    echo "Interactive confirmation skipped with --yes."
    return
  fi

  section "Destructive confirmation"
  echo "This will permanently delete archived infrastructure and data from ${PROJECT_ID}."
  echo "Geocoding API, Dynamic Maps API, key ${PROTECTED_KEY_ID}, map-id, and maps-api-key are preserved."
  echo
  printf 'Type exactly "DELETE efes-md EXCEPT MAPS": '
  read -r confirmation
  if [ "${confirmation}" != "DELETE efes-md EXCEPT MAPS" ]; then
    echo "Confirmation did not match. Nothing was deleted."
    exit 1
  fi
}

delete_gke_backup() {
  section "Delete GKE backups"

  local backup_names
  backup_names="$(gcloud beta container backup-restore backups list \
    --backup-plan=catalina-backup-1 \
    --location="${REGION}" \
    --project="${PROJECT_ID}" \
    --format=value\(name.basename\(\)\) 2>/dev/null || true)"

  local backup_name
  for backup_name in ${backup_names}; do
    run gcloud beta container backup-restore backups delete "${backup_name}" \
      --backup-plan=catalina-backup-1 \
      --location="${REGION}" \
      --project="${PROJECT_ID}" \
      --quiet
  done

  run gcloud beta container backup-restore backup-plans delete catalina-backup-1 \
    --location="${REGION}" \
    --project="${PROJECT_ID}" \
    --quiet
}

delete_compute() {
  section "Delete persistent disks"
  run gcloud compute disks delete \
    pvc-05741d07-2cd7-45f0-9cda-bd9486aa1a1d \
    pvc-08d1f464-ea6a-4f1d-97d6-0c969d9c40c6 \
    pvc-136281be-e293-408f-b3ed-ba9959aa95c9 \
    pvc-27bde7fb-f92f-46c9-812b-3329b2836386 \
    pvc-2ab9272e-22fc-4225-b30a-719aadbd1abc \
    pvc-5eb80f23-e2e7-4a65-92f3-5308e41699a9 \
    pvc-9f90f2ff-e20b-42ac-85ee-fcb5afedb343 \
    pvc-a1e9016d-135f-455e-8d31-4419348b9867 \
    pvc-ab29f926-6d81-4b66-bcee-abeddd26c348 \
    pvc-ba55a740-9c37-49ed-9c12-05ab687bdeaf \
    pvc-c7ff5591-fc66-4643-a382-a8c9149d3c4a \
    pvc-cbbb1f98-eae9-4295-9dec-eabd7be42ecc \
    pvc-e59d901b-a00f-485b-b2da-1c44dcf60b07 \
    pvc-f3d30daf-730a-4aae-8c76-0e57e635e8df \
    pvc-f8f7f7a1-9757-4a54-8275-bff99e4b4f0c \
    --zone="${ZONE}" \
    --project="${PROJECT_ID}" \
    --quiet

  section "Delete snapshots"
  run gcloud compute snapshots delete \
    snapshot-779209aa-eac6-40a9-b1c4-a5f8c00d47c4 \
    snapshot-d731026f-1ea3-4ea5-b86f-46280734d5dd \
    --project="${PROJECT_ID}" \
    --quiet
}

delete_network() {
  section "Delete Cloud NAT and router"
  run gcloud compute routers nats delete gke-outward-traffic \
    --router=gke-router \
    --region="${REGION}" \
    --project="${PROJECT_ID}" \
    --quiet

  run gcloud compute routers delete gke-router \
    --region="${REGION}" \
    --project="${PROJECT_ID}" \
    --quiet

  section "Delete external IP addresses"
  run gcloud compute addresses delete gke-outward-ip gw \
    --region="${REGION}" \
    --project="${PROJECT_ID}" \
    --quiet

  run gcloud compute addresses delete \
    dev-ingress-ip md-ingress-ip md-qa-ingress-ip tr-ingress-ip tr-qa-ingress-ip \
    --global \
    --project="${PROJECT_ID}" \
    --quiet

  section "Delete failed SSL certificates"
  run gcloud compute ssl-certificates delete \
    mcrt-223d3a8f-e4e6-43b7-8284-d901f18cb01d \
    mcrt-339949ef-a46c-496f-9635-9be066dba445 \
    mcrt-6b2e37f6-cebb-40fb-ae7e-9609db87f55a \
    mcrt-888bc6f5-1bb2-4173-9468-de1b14ae175e \
    mcrt-942ff121-ff3a-4ac9-b15f-ae3b1fb79ae6 \
    mcrt-a3d7ffb7-6691-4ab8-8912-bc22eacde4d0 \
    mcrt-baa300c0-5d1a-404b-a965-13e109f3d1fa \
    mcrt-cc3e010c-2ade-48ec-93c6-84aa7177318d \
    --global \
    --project="${PROJECT_ID}" \
    --quiet

  section "Delete VPC"
  run gcloud compute networks subnets delete europe-central2-proxy-only \
    --region="${REGION}" \
    --project="${PROJECT_ID}" \
    --quiet

  run gcloud compute networks delete default \
    --project="${PROJECT_ID}" \
    --quiet
}

delete_artifacts() {
  section "Delete Artifact Registry"
  run gcloud artifacts repositories delete c \
    --location="${REGION}" \
    --project="${PROJECT_ID}" \
    --quiet

  run gcloud artifacts repositories delete gcf-artifacts \
    --location="${REGION}" \
    --project="${PROJECT_ID}" \
    --quiet
}

delete_storage() {
  section "Delete Cloud Storage buckets"

  local bucket
  for bucket in \
    atlas-file-upload-efes-md \
    dagster-efes-compute-logs \
    efes-md-backup \
    gcf-v2-sources-183284693154-europe-central2 \
    gcf-v2-uploads-183284693154-europe-central2 \
    gloksi-b2b \
    maya-backup \
    maya-files-dev \
    maya-files-md \
    maya-files-md-qa \
    maya-files-tr \
    maya-files-tr-qa
  do
    run gcloud storage rm --recursive "gs://${bucket}/" --project="${PROJECT_ID}"
  done
}

cleanup_api_keys() {
  section "Delete obsolete API keys"

  local key_id
  for key_id in \
    d2faed3f-e820-44b0-b997-50d09254ecb1 \
    28a38507-8996-46b6-bed9-05d536655940 \
    b7267bb8-6962-4b13-82a4-ec4d0d2cf02a \
    2abc20c3-3cf5-4539-9607-65c42084ff97 \
    39a7363d-7f7e-45a2-b31d-79000b9b2c38
  do
    run gcloud services api-keys delete "${key_id}" \
      --project="${PROJECT_ID}" \
      --quiet
  done

  section "Restrict atlas key to Dynamic Maps only"
  run gcloud services api-keys update \
    dc5e6e7b-6627-4182-8a44-5bf093589209 \
    --api-target=service=maps-backend.googleapis.com \
    --project="${PROJECT_ID}"
}

delete_secrets() {
  section "Delete non-Maps secrets"

  local secret
  for secret in \
    appflow-ionic \
    bono-conn \
    bono-h-admin-secret \
    bono-marketing-secret \
    bono-telegram-token \
    caps-hasura-admin-secret-md \
    cf-key \
    dev-file-link \
    efes-md-sms-pass \
    firebase-token \
    hasura-conn \
    jwt-private-key \
    mg-key \
    notify-conn \
    notify-run-cf-key \
    notify-secret \
    npm-token \
    npm-token-readonly \
    sap-inbound-keys \
    sap-order-service-login \
    sap-order-service-pass \
    sentry-auth-token \
    smart-conn-master \
    smart-conn-slave \
    smart-hasura-admin-secret \
    x-nitra-cf-key
  do
    run gcloud secrets delete "${secret}" \
      --project="${PROJECT_ID}" \
      --quiet
  done
}

delete_service_accounts() {
  section "Delete obsolete service accounts"

  local service_account
  for service_account in \
    github-actions@efes-md.iam.gserviceaccount.com \
    firebase-adminsdk-6gnni@efes-md.iam.gserviceaccount.com \
    firebase-deploy@efes-md.iam.gserviceaccount.com \
    cnpg-backup-sa@efes-md.iam.gserviceaccount.com \
    fcm-sender@efes-md.iam.gserviceaccount.com \
    gsa-file-link-shared@efes-md.iam.gserviceaccount.com \
    efes-md@appspot.gserviceaccount.com \
    183284693154-compute@developer.gserviceaccount.com
  do
    run gcloud iam service-accounts delete "${service_account}" \
      --project="${PROJECT_ID}" \
      --quiet
  done
}

disable_service() {
  local service="$1"
  case " ${PROTECTED_SERVICES} " in
    *" ${service} "*)
      echo "REFUSED: attempted to disable protected service ${service}"
      FAILURES=$((FAILURES + 1))
      return
      ;;
  esac

  run gcloud services disable "${service}" \
    --project="${PROJECT_ID}" \
    --quiet
}

disable_obsolete_services() {
  section "Disable obsolete APIs"

  local service
  for service in \
    appengine.googleapis.com \
    cloudfunctions.googleapis.com \
    run.googleapis.com \
    cloudbuild.googleapis.com \
    source.googleapis.com \
    runtimeconfig.googleapis.com \
    pubsub.googleapis.com \
    secretmanager.googleapis.com \
    datastore.googleapis.com \
    identitytoolkit.googleapis.com \
    securetoken.googleapis.com \
    fcm.googleapis.com \
    fcmregistrations.googleapis.com \
    firebase.googleapis.com \
    firebaseappdistribution.googleapis.com \
    firebaseapptesters.googleapis.com \
    firebasehosting.googleapis.com \
    firebaseinappmessaging.googleapis.com \
    firebaseinstallations.googleapis.com \
    firebaseremoteconfig.googleapis.com \
    firebaseremoteconfigrealtime.googleapis.com \
    firebaserules.googleapis.com \
    firebasedynamiclinks.googleapis.com \
    mobilecrashreporting.googleapis.com \
    gkebackup.googleapis.com \
    containerfilesystem.googleapis.com \
    container.googleapis.com \
    autoscaling.googleapis.com \
    networkconnectivity.googleapis.com \
    oslogin.googleapis.com \
    certificatemanager.googleapis.com \
    dns.googleapis.com \
    artifactregistry.googleapis.com \
    containerregistry.googleapis.com \
    storage-api.googleapis.com \
    storage-component.googleapis.com \
    storage.googleapis.com \
    compute.googleapis.com \
    analyticshub.googleapis.com \
    bigqueryconnection.googleapis.com \
    bigquerydatapolicy.googleapis.com \
    bigquerydatatransfer.googleapis.com \
    bigquerymigration.googleapis.com \
    bigqueryreservation.googleapis.com \
    bigquerystorage.googleapis.com \
    bigquery.googleapis.com \
    dataform.googleapis.com \
    dataplex.googleapis.com \
    appoptimize.googleapis.com \
    cloudtrace.googleapis.com \
    cloudaicompanion.googleapis.com \
    geminicloudassist.googleapis.com \
    recommender.googleapis.com \
    telemetry.googleapis.com \
    testing.googleapis.com \
    sql-component.googleapis.com \
    directions-backend.googleapis.com
  do
    disable_service "${service}"
  done
}

final_diagnostics() {
  section "Final protected-resource verification"
  run_required gcloud services api-keys describe "${PROTECTED_KEY_ID}" \
    --project="${PROJECT_ID}" \
    --format=yaml\(displayName\,restrictions\)

  local service
  local enabled_services
  enabled_services="$(gcloud services list --enabled --project="${PROJECT_ID}" --format=value\(config.name\))"
  for service in ${PROTECTED_SERVICES}; do
    if printf '%s\n' "${enabled_services}" | grep -qx "${service}"; then
      echo "OK: protected service remains enabled: ${service}"
    else
      echo "ERROR: protected service is not enabled: ${service}"
      FAILURES=$((FAILURES + 1))
    fi
  done

  section "Remaining Cloud assets"
  run gcloud asset search-all-resources \
    --scope="projects/${PROJECT_ID}" \
    --billing-project="${PROJECT_ID}" \
    --limit=1000 \
    --format=table\(assetType.basename\(\)\,displayName\,location\,state\)

  section "Remaining API keys"
  run gcloud services api-keys list --project="${PROJECT_ID}"

  section "Remaining enabled APIs"
  run gcloud services list --enabled --project="${PROJECT_ID}"
}

section "efes-md cleanup"
echo "Mode: ${MODE}"
echo "Project: ${PROJECT_ID}"
echo "Log: ${LOG_FILE}"

preflight

if [ "${MODE}" = "diagnose" ]; then
  diagnose
  section "Diagnostic result"
  echo "Read-only diagnostics complete."
  echo "Failures: ${FAILURES}"
  echo "Log: ${LOG_FILE}"
  exit "${FAILURES}"
fi

confirm_cleanup
delete_gke_backup
delete_compute
delete_network
delete_artifacts
delete_storage
cleanup_api_keys
delete_secrets
delete_service_accounts
disable_obsolete_services
final_diagnostics

section "Cleanup result"
echo "Failures: ${FAILURES}"
echo "Log: ${LOG_FILE}"

if [ "${FAILURES}" -ne 0 ]; then
  echo "Cleanup completed with errors. Send this log back to Codex for analysis."
  exit 1
fi

echo "Cleanup completed successfully."
