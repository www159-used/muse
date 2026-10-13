Feature: Sequential playlist advance

  Scenario: Next walks the playlist and stops after the last track
    Given "downloads/a.mp3" is a copy of the fixture "silence.mp3"
    And "downloads/b.flac" is a copy of the fixture "tone.flac"

    When I import the folder "downloads"
    And I play the playlist "全部本地"
    And I advance to the next track

    Then the current playlist is "全部本地"
    And the playback mode is "sequential"
    And the current track index is 2 of 2

    When I try to advance past the last track

    Then advancing fails
    And the current track index is 2 of 2
    And the playback mode is "sequential"
