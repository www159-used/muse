Feature: Next track on the current playlist

  Scenario: Status exposes the upcoming track while playing 全部本地
    Given "downloads/a.mp3" is a copy of the fixture "silence.mp3"
    And "downloads/b.flac" is a copy of the fixture "tone.flac"

    When I import the folder "downloads"
    And I play the playlist "全部本地"

    Then the current playlist is "全部本地"
    And the current track index is 1 of 2
    And the next track index is 2 of 2
    And the next track title is the title of fixture "tone.flac"

  Scenario: Sequential playback in the middle of the playlist exposes the following track
    Given "downloads/a.mp3" is a copy of the fixture "silence.mp3"
    And "downloads/b.flac" is a copy of the fixture "tone.flac"
    And "downloads/c.mp3" is a copy of the fixture "tagged.mp3"
    And "downloads/d.mp3" is a copy of the fixture "other-artist.mp3"

    When I import the folder "downloads"
    And I play the track from fixture "tone.flac"

    Then the current playlist is "全部本地"
    And the playback mode is "sequential"
    And the current track title is the title of fixture "tone.flac"
    And the current track index is 2 of 4
    And the next track index is 3 of 4
    And the next track title is the title of fixture "tagged.mp3"

    When I advance to the next track

    Then the current track index is 3 of 4
    And the next track index is 4 of 4
    And the next track title is the title of fixture "other-artist.mp3"

    When I advance to the next track

    Then the current track index is 4 of 4
    And there is no next track

  Scenario: After the last track in sequential mode there is no next track
    Given "downloads/a.mp3" is a copy of the fixture "silence.mp3"
    And "downloads/b.flac" is a copy of the fixture "tone.flac"

    When I import the folder "downloads"
    And I play the playlist "全部本地"
    And I advance to the next track

    Then the current track index is 2 of 2
    And there is no next track

  Scenario: Play-next-once stays on the same playlist
    Given "downloads/a.mp3" is a copy of the fixture "silence.mp3"
    And "downloads/b.flac" is a copy of the fixture "tone.flac"
    And "downloads/c.mp3" is a copy of the fixture "tagged.mp3"

    When I import the folder "downloads"
    And I play the playlist "全部本地"
    And I queue the track from fixture "tagged.mp3" to play next once

    Then the current playlist is "全部本地"
    And the current track index is 1 of 3
    And the next track index is 3 of 3
    And the next track title is the title of fixture "tagged.mp3"

    When I advance to the next track

    Then the current playlist is "全部本地"
    And the current track title is the title of fixture "tagged.mp3"
    And the next track index is 2 of 3
    And the next track title is the title of fixture "tone.flac"

    When I advance to the next track

    Then the current playlist is "全部本地"
    And the current track index is 2 of 3
    And the current track title is the title of fixture "tone.flac"
