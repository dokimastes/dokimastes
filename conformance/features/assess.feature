Feature: Substrate assessment
  A codebase gets a green, amber or red verdict on whether it can support
  agentic delivery. Every rating cites where its value came from and what
  would have to change. Unknown is the most restrictive value. A profile
  that claims more than the measurement supports is refused, not warned.

  Scenario: Everything in order is green with m3-session
    Given a fully qualified profile on a well-formed tree
    When dok assesses the substrate
    Then the verdict is green
    And the mode ceiling is "m3-session"
    And the profile is not refused

  Scenario: A slow inner loop caps the mode at m3-staged
    Given a fully qualified profile on a well-formed tree
    And the inner loop p95 is 12.0 minutes
    When dok assesses the substrate
    Then the verdict is amber
    And the mode ceiling is "m3-staged"

  Scenario: Nothing known is red, because unknown is the most restrictive value
    Given an empty profile on an empty tree
    When dok assesses the substrate
    Then the verdict is red
    And the mode ceiling is "D2 only (m2-*)"
    And every finding that is not ok names what would have to change

  Scenario: No F3 boundary is red however good the rest
    Given a fully qualified profile on a well-formed tree
    And required checks cannot be set by anyone other than the developers
    When dok assesses the substrate
    Then the verdict is red

  Scenario: A profile claiming more than the measurement supports is refused
    Given a fully qualified profile on a well-formed tree
    And the inner loop p95 is 12.0 minutes
    And the profile declares substrate green
    And the profile declares default_mode m3-session
    When dok assesses the substrate
    Then the profile is refused because "profile declares substrate green"
    And the profile is refused because "exceeds the ceiling"

  Scenario: A profile claiming less than the measurement supports is fine
    Given a fully qualified profile on a well-formed tree
    And the profile declares substrate amber
    And the profile declares default_mode m2-review
    When dok assesses the substrate
    Then the profile is not refused

  Scenario: D4 is never admitted by substrate assessment
    Given a fully qualified profile on a well-formed tree
    And the profile declares default_mode m4-flagged-rollout
    When dok assesses the substrate
    Then the verdict is green
    And the profile is refused because "D4"

  Scenario: An area with no independent check gets no lane above 3
    Given a workload reporting-ui with oracle class none
    Then the oracle consequence for reporting-ui contains "no lane above 3"

  Scenario: Tests the agent wrote never qualify a change for reduced review
    Given a workload billing with oracle class agent-tests
    Then the oracle consequence for billing contains "never qualifies"
