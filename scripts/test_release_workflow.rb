#!/usr/bin/env ruby
# frozen_string_literal: true

workflow = File.read(
  File.expand_path("../.github/workflows/release.yml", __dir__),
  encoding: "UTF-8"
)

release_match = workflow.match(
  /^  release:\n(?<body>.*?)(?=^  [a-zA-Z0-9_]+:\n|\z)/m
)
abort("release workflow is missing the release job") unless release_match

release = release_match[0]

required_fragments = [
  "cargo-cyclonedx --version 0.5.9 --locked",
  "mcp-doctor-${RELEASE_TAG}.cdx.json",
  "actions/attest@",
  "subject-path:",
  "sbom-path:",
  'gh release create "${RELEASE_TAG}" "${assets[@]}"',
  "--paginate --slurp",
  "Replacing interrupted draft",
  "refusing to overwrite published release",
  "Remote release asset inventory is not exact",
  "Remote release asset digest does not match",
  'repos/${GH_REPO}/releases/${RELEASE_ID}',
  '.draft == true and .tag_name == $tag',
  "-F draft=false"
]

required_fragments.each do |fragment|
  abort("release workflow is missing #{fragment.inspect}") unless workflow.include?(fragment)
end

action_references = workflow.scan(/^\s*uses:\s*([^@\s]+)@([^\s#]+)/).map do |name, revision|
  [name, revision]
end
unpinned_actions = action_references.reject { |_name, revision| revision.match?(/\A[0-9a-f]{40}\z/) }
unless unpinned_actions.empty?
  abort("release workflow has unpinned actions: #{unpinned_actions.map(&:first).join(', ')}")
end

abort("draft releases must be published by release id") if workflow.include?("gh release edit")
abort("release assets must be uploaded during draft creation") if release.include?("gh release upload")

puts "release workflow contract passed"
