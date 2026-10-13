Feature: Previous track and transport commands

  Scenario: Prev steps back and refuses to wrap the first sequential track
    Given "downloads/a.mp3" is a copy of the fixture "silence.mp3"
    And "downloads/b.flac" is a copy of the fixture "tone.flac"

    When I import the folder "downloads"
    And I play the playlist "全部本地"

    Then the current track index is 1 of 2
    And the next track index is 2 of 2
    And the playback state is "Playing"
    And the status has no previous-track field

    When I try to go back past the first track

    Then going back fails
    And the current track index is 1 of 2
    And the next track index is 2 of 2
    And the playback state is "Playing"

    When I advance to the next track

    Then the current track index is 2 of 2
    And there is no next track

    When I go to the previous track

    Then the current track index is 1 of 2
    And the next track index is 2 of 2
    And the status has no previous-track field

  Scenario: Pause and play change the playback state
    Given "downloads/a.mp3" is a copy of the fixture "silence.mp3"
    And "downloads/b.flac" is a copy of the fixture "tone.flac"

    When I import the folder "downloads"
    And I play the playlist "全部本地"

    Then the playback state is "Playing"

    When I pause playback
    Then the playback state is "Paused"

    When I pause playback
    Then the playback state is "Paused"

    When I resume playback
    Then the playback state is "Playing"

  Scenario: List loop wraps previous to the last track
    Given "downloads/a.mp3" is a copy of the fixture "silence.mp3"
    And "downloads/b.flac" is a copy of the fixture "tone.flac"

    When I import the folder "downloads"
    And I play the playlist "全部本地"
    And I cycle the playback mode

    Then the playback mode is "list loop"
    And the current track index is 1 of 2

    When I go to the previous track

    Then the current track index is 2 of 2
    And the next track index is 1 of 2
    And the status has no previous-track field

  Scenario: Transport commands fail when the playback service is down
    Given "downloads/a.mp3" is a copy of the fixture "silence.mp3"

    When I import the folder "downloads"
    And I play the playlist "全部本地"
    And I shut down the playback service

    Then transport commands fail because playback is not running
