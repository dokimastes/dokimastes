Feature: Negative-capability probes
  Every forbidden action is tried for real, by the dok binary, against a
  git server. Before protection every attempt goes through and is
  restored. After protection every attempt is refused and credited to the
  mechanism whose wording appears in the server's response. A refusal
  nobody named is a finding. A probe that was not run is unproven, never
  satisfied.

  Background:
    Given a git server with a repository holding branches main and side

  Scenario Outline: Before protection every attempt goes through and is restored
    When dok conform runs the <probe> probe as agent expecting red
    Then the verdict is pass
    And the outcome is succeeded and the previous state was restored
    And the repository is unchanged
    And the exit code is 0

    Examples:
      | probe         |
      | force-push    |
      | delete-branch |
      | direct-push   |
      | push-tag      |

  Scenario Outline: After protection every attempt is refused and credited
    Given the server refuses every push with "GH013: Repository rule violations found"
    When dok conform runs the <probe> probe as agent expecting green
    Then the verdict is pass
    And the outcome is refused by "ruleset"
    And the repository is unchanged
    And the exit code is 0

    Examples:
      | probe         |
      | force-push    |
      | delete-branch |
      | direct-push   |
      | push-tag      |

  Scenario: A refusal before protection is a failed rehearsal
    Given the server refuses every push with "GH013: Repository rule violations found"
    When dok conform runs the force-push probe as agent expecting red
    Then the verdict is fail
    And the note contains "failed rehearsal"
    And the exit code is 1

  Scenario: An attempt that goes through after protection fails the run
    When dok conform runs the force-push probe as agent expecting green
    Then the verdict is fail
    And the repository is unchanged
    And the exit code is 1

  Scenario: A refusal the pack did not name is a finding, not a credit
    Given the server refuses every push with "no pushes on Fridays"
    When dok conform runs the force-push probe as agent expecting green
    Then the verdict is pass
    And the outcome is refused by an unidentified mechanism
    And the note contains "finding"

  Scenario: Deleting the default branch is stopped by git itself, not by a rule
    When dok conform runs the delete-main probe as agent expecting green
    Then the outcome is refused by "default-branch guard"

  Scenario: A probe run under the wrong identity is unproven, not satisfied
    When dok conform runs the admin-only probe as agent expecting green
    Then the outcome is not-run
    And the verdict is unproven
    And the exit code is 1
