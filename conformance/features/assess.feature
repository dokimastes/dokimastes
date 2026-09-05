Feature: Substrate assessment
  The dok binary assesses a working tree and a profile and gives a green,
  amber or red verdict on whether the codebase can support agentic
  delivery. Every rating cites where its value came from and what would
  have to change. Unknown is the most restrictive value. A profile that
  claims more than the measurement supports is refused, not warned.

  Background:
    Given a git server with a repository holding branches main and side

  Scenario: Everything in order is green with m3-session
    Given the working tree contains "build.gradle, Dockerfile, CODEOWNERS"
    And a profile with
      """
      id: acme
      ci: { inner_loop: "./gradlew check", inner_loop_p95_minutes: 4, flake_rate_30d: 0.9% }
      verdict_inputs: { mutation: pitest }
      assessment:
        cold_build_command: "./gradlew build"
        test_green_on_main: true
        required_checks_settable_by_non_developers: true
        mutation_score: 61
      """
    When dok assess runs on the working tree with the profile
    Then the verdict is green
    And the mode ceiling is "m3-session"
    And the profile is not refused
    And the exit code is 0

  Scenario: A slow inner loop caps the mode at m3-staged
    Given the working tree contains "build.gradle, Dockerfile, CODEOWNERS"
    And a profile with
      """
      id: acme
      ci: { inner_loop: "./gradlew check", inner_loop_p95_minutes: 12, flake_rate_30d: 0.9% }
      verdict_inputs: { mutation: pitest }
      assessment:
        cold_build_command: "./gradlew build"
        test_green_on_main: true
        required_checks_settable_by_non_developers: true
        mutation_score: 61
      """
    When dok assess runs on the working tree with the profile
    Then the verdict is amber
    And the mode ceiling is "m3-staged"

  Scenario: Nothing known is red, because unknown is the most restrictive value
    When dok assess runs on the working tree without a profile
    Then the verdict is red
    And the mode ceiling is "D2 only (m2-*)"
    And every finding that is not ok names what would have to change
    And the exit code is 0

  Scenario: No F3 boundary is red however good the rest
    Given the working tree contains "build.gradle, Dockerfile, CODEOWNERS"
    And a profile with
      """
      id: acme
      ci: { inner_loop: "./gradlew check", inner_loop_p95_minutes: 4, flake_rate_30d: 0.9% }
      verdict_inputs: { mutation: pitest }
      assessment:
        cold_build_command: "./gradlew build"
        test_green_on_main: true
        required_checks_settable_by_non_developers: false
        mutation_score: 61
      """
    When dok assess runs on the working tree with the profile
    Then the verdict is red
    And the finding "branch-protection" is blocking

  Scenario: A profile claiming more than the measurement supports is refused
    Given a profile with
      """
      id: acme
      substrate: green
      default_mode: m3-session
      """
    When dok assess runs on the working tree with the profile
    Then the verdict is red
    And the profile is refused because "profile declares substrate green"
    And the profile is refused because "exceeds the ceiling"
    And the exit code is 1

  Scenario: D4 is never admitted by substrate assessment
    Given a profile with
      """
      id: acme
      default_mode: m4-flagged-rollout
      """
    When dok assess runs on the working tree with the profile
    Then the profile is refused because "D4"
    And the exit code is 1

  Scenario: An area with no independent check gets no lane above 3
    Given a profile with
      """
      id: acme
      oracles:
        - { workload: reporting-ui, class: none }
        - { workload: billing, class: agent-tests }
      """
    When dok assess runs on the working tree with the profile
    Then the oracle consequence for reporting-ui contains "no lane above 3"
    And the oracle consequence for billing contains "never qualifies"

  Scenario: A profile with a field the schema does not know is refused
    Given a profile with
      """
      id: acme
      substrate_override: green
      """
    When dok assess runs on the working tree with the profile
    Then dok exits with code 2 and reports "unknown field"
