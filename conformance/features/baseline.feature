Feature: Baseline
  Before the first agent runs, the dok binary captures the before number
  from the repository's own history: every throughput figure paired with
  the degradation figure the evidence says moves against it. What history
  cannot yield is named, never left blank. A baseline captured after the
  first agent run is not a before number and is refused.

  Background:
    Given a git server with a repository holding branches main and side
    And a repository with a known month of history

  Scenario: The figures history can yield are computed and paired
    When dok baseline runs over the last 60 days
    Then the metric "commits per week" is 0.58
    And the metric "code churn within 14 days" is 24.79
    And the metric "revert commits" is 1
    And the metric "median merge size" is 20
    And the metric "median lead time" is 48
    And the metric "releases per week" is 0.12
    And the exit code is 0

  Scenario: What history cannot yield is named, not left blank
    When dok baseline runs over the last 60 days
    Then the metric "merges with no review" is not recoverable because of platform-api
    And the metric "mutation score" is not recoverable because of tooling
    And the metric "escaped-defect rate per lane" is not recoverable because of collect-forward
    And no person is named in the report

  Scenario: A baseline captured after the first agent run is refused
    When dok baseline runs over the last 60 days with the first agent run on 2026-01-01
    Then the baseline is refused because "not a before number"
    And the exit code is 1

  Scenario: A baseline captured before the first agent run stands
    When dok baseline runs over the last 60 days with the first agent run on 2999-01-01
    Then the baseline is not refused
    And the exit code is 0

  Scenario: A window that misses the history is empty, not wrong
    When dok baseline runs over the last 1 days
    Then the metric "commits per week" is 0
    And the metric "code churn within 14 days" has no value
    And the exit code is 0
