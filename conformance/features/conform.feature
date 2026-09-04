Feature: Negative-capability probes
  Every forbidden action is tried for real. Before protection every attempt
  goes through and is restored. After protection every attempt is refused
  and credited to the mechanism whose wording appears in the response. A
  refusal nobody named is a finding. A probe that was not run is unproven,
  never satisfied.

  Background:
    Given a local repository with branches main and side

  Scenario Outline: Before protection every attempt goes through and is restored
    When the <attempt> attempt is tried
    Then the attempt went through and the previous state was restored
    And under expectation red the verdict is pass
    And under expectation green the verdict is fail

    Examples:
      | attempt       |
      | force-push    |
      | delete-branch |
      | direct-push   |
      | push-tag      |

  Scenario Outline: After protection every attempt is refused and credited
    Given the remote refuses every push with "GH013: Repository rule violations found"
    When the <attempt> attempt is tried
    Then the attempt was refused by "ruleset"
    And under expectation green the verdict is pass
    And under expectation red the verdict is fail

    Examples:
      | attempt       |
      | force-push    |
      | delete-branch |
      | direct-push   |
      | push-tag      |

  Scenario: A refusal the pack did not name is a finding, not a credit
    Given the remote refuses every push with "no pushes on Fridays"
    When the force-push attempt is tried
    Then the attempt was refused by an unidentified mechanism
    And under expectation green the verdict is pass with a finding

  Scenario: Deleting the default branch is stopped by git itself, not by a rule
    When the delete-branch attempt is tried on main
    Then the attempt was refused by "default-branch guard"

  Scenario: A probe run under the wrong identity is unproven, not satisfied
    Given a probe meaningful only as repo-admin
    When it is run as agent
    Then the probe was not run
    And under expectation green the verdict is unproven
    And under expectation red the verdict is unproven
