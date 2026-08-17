#!/usr/bin/env ruby
# frozen_string_literal: true

root = File.expand_path("..", __dir__)
workflow = File.read(File.join(root, ".github/workflows/release.yml"), encoding: "UTF-8")
ci_workflow = File.read(File.join(root, ".github/workflows/ci.yml"), encoding: "UTF-8")
release_script = File.read(File.join(root, "scripts/publish_release.sh"), encoding: "UTF-8")

release_match = workflow.match(
  /^  release:\n(?<body>.*?)(?=^  [a-zA-Z0-9_]+:\n|\z)/m
)
abort("release workflow is missing the release job") unless release_match

release = release_match[0]

required_workflow_fragments = [
  "cargo-cyclonedx --version 0.5.9 --locked",
  "mcp-doctor-${RELEASE_TAG}.cdx.json",
  "actions/attest@",
  "subject-path:",
  "sbom-path:",
  "bash scripts/publish_release.sh create release-assets",
  'bash scripts/publish_release.sh publish release-assets "${RELEASE_ID}"',
  'bash scripts/publish_release.sh cleanup release-assets "${RELEASE_ID:-}"'
]

required_workflow_fragments.each do |fragment|
  abort("release workflow is missing #{fragment.inspect}") unless workflow.include?(fragment)
end

required_script_fragments = [
  '<!-- tinkora-release-owner:${GH_REPO}:${RELEASE_TAG}:${GITHUB_SHA} -->',
  '<!-- tinkora-release-run:${GITHUB_RUN_ID}:${GITHUB_RUN_ATTEMPT} -->',
  'repos/${GH_REPO}/git/ref/tags/${RELEASE_TAG}',
  'repos/${GH_REPO}/git/tags/${object_sha}',
  "release tag annotation chain exceeds 16 objects",
  "not owned by this release workflow",
  "Remote release asset inventory is not exact",
  "Remote release asset digest does not match",
  'repos/${GH_REPO}/releases/${release_id}'
]

required_script_fragments.each do |fragment|
  abort("release script is missing #{fragment.inspect}") unless release_script.include?(fragment)
end

[
  "shellcheck scripts/publish_release.sh",
  "ruby scripts/test_publish_release.rb"
].each do |fragment|
  abort("CI workflow is missing #{fragment.inspect}") unless ci_workflow.include?(fragment)
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
abort("release job must delegate draft creation to the tested script") if release.include?("gh release create")
abort("release job must delegate publication to the tested script") if release.include?("--method PATCH")

puts "release workflow contract passed"
