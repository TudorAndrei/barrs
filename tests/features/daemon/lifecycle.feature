Feature: Control the daemon over its socket
  The barrs commands talk to the running daemon through its Unix socket.
  A bad request gets an error, and the daemon continues to run.

  Background:
    Given a running daemon with a clock item that records its clicks

  Scenario: The daemon answers a ping and stops on request
    When I run "barrs ping"
    Then the command prints "pong"
    When I run "barrs stop"
    Then the command succeeds
    And the daemon exits

  Scenario: A malformed request gets an error
    When a client sends the request line "not json"
    Then the daemon answers with an error
    And the daemon answers a ping

  Scenario: A reload with a broken config keeps the loaded config
    Given the config file now contains "this is not lua"
    When I run "barrs reload"
    Then the command fails
    And the daemon answers a ping
