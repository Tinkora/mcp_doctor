#!/usr/bin/env bash
set -euo pipefail

mode="${1:-}"
asset_directory="${2:-}"
requested_release_id="${3:-}"

: "${GH_REPO:?GH_REPO is required}"
: "${RELEASE_TAG:?RELEASE_TAG is required}"
: "${GITHUB_SHA:?GITHUB_SHA is required}"
: "${GITHUB_RUN_ID:?GITHUB_RUN_ID is required}"
: "${GITHUB_RUN_ATTEMPT:?GITHUB_RUN_ATTEMPT is required}"

[[ "${RELEASE_TAG}" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
  echo "release tag must use vMAJOR.MINOR.PATCH" >&2
  exit 1
}
[[ "${GITHUB_SHA}" =~ ^[0-9a-f]{40}$ ]] || {
  echo "GITHUB_SHA must be a full commit digest" >&2
  exit 1
}

owner_marker="<!-- tinkora-release-owner:${GH_REPO}:${RELEASE_TAG}:${GITHUB_SHA} -->"
run_marker="<!-- tinkora-release-run:${GITHUB_RUN_ID}:${GITHUB_RUN_ATTEMPT} -->"

resolve_remote_tag_commit() {
  local ref_json object_type object_sha tag_json
  ref_json="$(gh api "repos/${GH_REPO}/git/ref/tags/${RELEASE_TAG}")"
  object_type="$(jq -er '.object.type' <<<"${ref_json}")"
  object_sha="$(jq -er '.object.sha' <<<"${ref_json}")"
  for ((depth = 0; depth < 16; depth++)); do
    case "${object_type}" in
      commit)
        printf '%s\n' "${object_sha}"
        return 0
        ;;
      tag)
        tag_json="$(gh api "repos/${GH_REPO}/git/tags/${object_sha}")"
        object_type="$(jq -er '.object.type' <<<"${tag_json}")"
        object_sha="$(jq -er '.object.sha' <<<"${tag_json}")"
        ;;
      *)
        echo "release tag resolves to unsupported object type ${object_type}" >&2
        return 1
        ;;
    esac
  done
  echo "release tag annotation chain exceeds 16 objects" >&2
  return 1
}

verify_remote_tag() {
  local remote_commit
  remote_commit="$(resolve_remote_tag_commit)"
  [[ "${remote_commit}" == "${GITHUB_SHA}" ]] || {
    echo "remote release tag does not identify the workflow commit" >&2
    return 1
  }
}

load_assets() {
  local asset
  [[ -d "${asset_directory}" ]] || {
    echo "release asset directory does not exist" >&2
    return 1
  }
  assets=()
  while IFS= read -r -d '' asset; do
    assets+=("${asset}")
  done < <(find "${asset_directory}" -maxdepth 1 -type f -print0 | LC_ALL=C sort -z)
  ((${#assets[@]} > 0)) || {
    echo "release asset directory is empty" >&2
    return 1
  }
}

list_releases() {
  gh api --paginate --slurp "repos/${GH_REPO}/releases?per_page=100"
}

verify_owned_draft() {
  local release_json="$1"
  jq -e --arg tag "${RELEASE_TAG}" --arg sha "${GITHUB_SHA}" --arg owner "${owner_marker}" '
    .tag_name == $tag
    and .target_commitish == $sha
    and .draft == true
    and ((.body // "") | contains($owner))
  ' <<<"${release_json}" >/dev/null
}

resolve_owned_draft() {
  local max_attempts="${1:-30}" attempt releases_json matching_releases match_count
  for ((attempt = 1; attempt <= max_attempts; attempt++)); do
    if ! releases_json="$(list_releases)"; then
      return 1
    fi
    matching_releases="$(jq -c \
      --arg tag "${RELEASE_TAG}" \
      --arg sha "${GITHUB_SHA}" \
      --arg owner "${owner_marker}" \
      --arg run "${run_marker}" '
        [.[][] | select(
          .tag_name == $tag
          and .target_commitish == $sha
          and .draft == true
          and ((.body // "") | contains($owner))
          and ((.body // "") | contains($run))
        )]
      ' <<<"${releases_json}")"
    match_count="$(jq -r 'length' <<<"${matching_releases}")"
    if [[ "${match_count}" == "1" ]]; then
      jq -er '.[0].id' <<<"${matching_releases}"
      return 0
    fi
    if ((match_count > 1)); then
      echo "multiple owned drafts use ${RELEASE_TAG}" >&2
      return 1
    fi
    ((attempt == max_attempts)) || sleep 1
  done
  echo "owned draft release cannot be resolved uniquely" >&2
  return 1
}

create_release() {
  local releases_json matching_releases match_count release_json release_id release_notes
  verify_remote_tag
  load_assets
  releases_json="$(list_releases)"
  matching_releases="$(jq -c --arg tag "${RELEASE_TAG}" \
    '[.[][] | select(.tag_name == $tag)]' <<<"${releases_json}")"
  match_count="$(jq -r 'length' <<<"${matching_releases}")"
  if ((match_count > 1)); then
    echo "multiple releases use ${RELEASE_TAG}" >&2
    return 1
  fi
  if ((match_count == 1)); then
    release_json="$(jq -c '.[0]' <<<"${matching_releases}")"
    release_id="$(jq -er '.id' <<<"${release_json}")"
    if ! jq -e '.draft == true' <<<"${release_json}" >/dev/null; then
      echo "refusing to overwrite published release ${release_id} for ${RELEASE_TAG}" >&2
      return 1
    fi
    if ! verify_owned_draft "${release_json}"; then
      echo "existing draft ${release_id} is not owned by this release workflow" >&2
      return 1
    fi
    release_json="$(gh api "repos/${GH_REPO}/releases/${release_id}")"
    if ! verify_owned_draft "${release_json}"; then
      echo "owned draft ${release_id} changed before replacement" >&2
      return 1
    fi
    echo "Replacing interrupted owned draft ${release_id} for ${RELEASE_TAG}."
    gh api --method DELETE "repos/${GH_REPO}/releases/${release_id}"
  fi

  release_notes="${owner_marker}"$'\n'"${run_marker}"
  gh release create "${RELEASE_TAG}" "${assets[@]}" \
    --repo "${GH_REPO}" \
    --verify-tag \
    --target "${GITHUB_SHA}" \
    --title "MCP Doctor ${RELEASE_TAG}" \
    --notes "${release_notes}" \
    --generate-notes \
    --draft >/dev/null
  release_id="$(resolve_owned_draft 30)" || return 1
  echo "release_id=${release_id}" >>"${GITHUB_OUTPUT}"
}

verify_release_identity() {
  local release_json="$1"
  jq -e \
    --arg tag "${RELEASE_TAG}" \
    --arg sha "${GITHUB_SHA}" \
    --arg owner "${owner_marker}" \
    --arg run "${run_marker}" '
      .tag_name == $tag
      and .target_commitish == $sha
      and .draft == true
      and .prerelease == false
      and ((.body // "") | contains($owner))
      and ((.body // "") | contains($run))
    ' <<<"${release_json}" >/dev/null
}

verify_remote_assets() {
  local release_json="$1" expected_names remote_names asset asset_name local_digest remote_digest
  expected_names="$(printf '%s\n' "${assets[@]}" | sed 's|.*/||' | LC_ALL=C sort)"
  remote_names="$(jq -r '.assets[].name' <<<"${release_json}" | LC_ALL=C sort)"
  if [[ "${remote_names}" != "${expected_names}" ]]; then
    echo "Remote release asset inventory is not exact." >&2
    return 1
  fi
  for asset in "${assets[@]}"; do
    asset_name="$(basename "${asset}")"
    local_digest="sha256:$(sha256sum -- "${asset}" | cut -d ' ' -f 1)"
    remote_digest="$(jq -r --arg name "${asset_name}" \
      '.assets[] | select(.name == $name) | .digest // empty' \
      <<<"${release_json}")"
    if [[ "${remote_digest}" != "${local_digest}" ]]; then
      echo "Remote release asset digest does not match ${asset_name}." >&2
      return 1
    fi
  done
}

verify_release_state() {
  local release_id="$1" release_json
  release_json="$(gh api "repos/${GH_REPO}/releases/${release_id}")"
  verify_release_identity "${release_json}" || {
    echo "release changed before publication" >&2
    return 1
  }
  verify_remote_assets "${release_json}"
}

publish_release() {
  local release_id="${requested_release_id}"
  [[ "${release_id}" =~ ^[0-9]+$ ]] || {
    echo "release ID is missing or invalid" >&2
    return 1
  }
  load_assets
  verify_remote_tag
  verify_release_state "${release_id}"
  verify_remote_tag
  verify_release_state "${release_id}"
  gh api --method PATCH "repos/${GH_REPO}/releases/${release_id}" \
    -F draft=false \
    -F prerelease=false >/dev/null
}

cleanup_release() {
  local release_id="${requested_release_id}" release_json
  if [[ ! "${release_id}" =~ ^[0-9]+$ ]]; then
    release_id="$(resolve_owned_draft 5 2>/dev/null)" || return 0
  fi
  release_json="$(gh api "repos/${GH_REPO}/releases/${release_id}" 2>/dev/null)" || return 0
  if jq -e \
    --arg tag "${RELEASE_TAG}" \
    --arg sha "${GITHUB_SHA}" \
    --arg owner "${owner_marker}" \
    --arg run "${run_marker}" '
      .tag_name == $tag
      and .target_commitish == $sha
      and .draft == true
      and ((.body // "") | contains($owner))
      and ((.body // "") | contains($run))
    ' <<<"${release_json}" >/dev/null; then
    gh api --method DELETE "repos/${GH_REPO}/releases/${release_id}"
  fi
}

case "${mode}" in
  create) create_release ;;
  publish) publish_release ;;
  cleanup) cleanup_release ;;
  *)
    echo "usage: publish_release.sh <create|publish|cleanup> <asset-directory> [release-id]" >&2
    exit 2
    ;;
esac
